//! Reads against the Gwei Name Service.
//!
//! GNS has no registry walk, no wildcards and no CCIP-read: `NameNFT` answers
//! the ENS resolver profile (`addr`, `text`, `contenthash`, ...) for every
//! node itself. All reads for one name are batched through Multicall3 so they
//! are served from one block, and that block's number and timestamp are read
//! in the same call so the cache knows how fresh the answer is.
//!
//! None of the `NameNFT` reads revert for unknown, expired or stale names:
//! they answer with the zero address or empty values.

use ethers::providers::{Http, Middleware, Provider, ProviderError};
use ethers_core::abi::{self, ParamType, Token};
use ethers_core::types::transaction::eip2718::TypedTransaction;
use ethers_core::types::{Address, Bytes, H256, U256};
use hex_literal::hex;
use thiserror::Error;
use tracing::instrument;

use crate::core::gns::{node_from_token, GweiName};
use crate::models::lookup::ENSLookup;

// Multicall3
/// `aggregate3((address,bool,bytes)[])`
const AGGREGATE3: [u8; 4] = hex!("82ad56cb");
/// `getBlockNumber()`
const GET_BLOCK_NUMBER: [u8; 4] = hex!("42cbb15c");
/// `getCurrentBlockTimestamp()`
const GET_CURRENT_BLOCK_TIMESTAMP: [u8; 4] = hex!("0f28c97d");

// NameNFT
/// `expiresAt(uint256)`
const EXPIRES_AT: [u8; 4] = hex!("17c95709");
/// `getFullName(uint256)`
const GET_FULL_NAME: [u8; 4] = hex!("465411c1");
/// `reverseResolve(address)`
const REVERSE_RESOLVE: [u8; 4] = hex!("9af8b7aa");
/// `primaryName(address)`
const PRIMARY_NAME: [u8; 4] = hex!("eba951aa");

#[derive(Error, Debug)]
pub enum GnsError {
    #[error("RPC error: {0}")]
    Rpc(#[from] ProviderError),

    #[error("Unexpected response to {0}")]
    Decode(&'static str),
}

/// Where the Gwei Name Service lives on the configured chain.
#[derive(Clone, Copy, Debug)]
pub struct GnsContracts {
    pub name_nft: Address,
    pub multicall: Address,
}

#[derive(Clone, Debug)]
pub struct CallResult {
    pub success: bool,
    pub data: Vec<u8>,
}

/// Everything `NameNFT` knows about a name at one block.
#[derive(Debug)]
pub struct NameSnapshot {
    /// Block the snapshot was read at.
    pub block: u64,
    /// Timestamp of that block.
    pub timestamp: u64,
    /// Expiry of the top-level name (subdomains have none of their own).
    pub expires_at: u64,
    /// One result per requested lookup, or `None` if the name does not
    /// currently resolve (unregistered, expired, or a stale subdomain).
    pub records: Option<Vec<CallResult>>,
}

/// The primary name of an address at one block.
#[derive(Debug)]
pub struct ReverseSnapshot {
    /// Block the snapshot was read at.
    pub block: u64,
    /// `NameNFT.reverseResolve`: empty unless the primary name is set, active
    /// and resolves back to the address.
    pub name: String,
    /// `NameNFT.primaryName`: the token the address points at, whether or not
    /// that currently yields a name.
    pub primary: Option<H256>,
}

fn word(selector: [u8; 4], word: &[u8]) -> Vec<u8> {
    let mut padded = [0u8; 32];
    padded[32 - word.len()..].copy_from_slice(word);

    [&selector as &[u8], &padded].concat()
}

async fn call(
    provider: &Provider<Http>,
    to: Address,
    data: Vec<u8>,
) -> Result<Bytes, ProviderError> {
    let mut transaction = TypedTransaction::default();

    transaction.set_to(to);
    transaction.set_data(Bytes::from(data));

    provider.call(&transaction, None).await
}

fn decode_one(result: &CallResult, kind: ParamType, what: &'static str) -> Result<Token, GnsError> {
    if !result.success {
        return Err(GnsError::Decode(what));
    }

    abi::decode(&[kind], &result.data)
        .ok()
        .and_then(|mut tokens| tokens.pop())
        .ok_or(GnsError::Decode(what))
}

fn decode_u64(result: &CallResult, what: &'static str) -> Result<u64, GnsError> {
    decode_one(result, ParamType::Uint(256), what)?
        .into_uint()
        .filter(|value| *value <= U256::from(u64::MAX))
        .map(|value| value.as_u64())
        .ok_or(GnsError::Decode(what))
}

fn decode_string(result: &CallResult, what: &'static str) -> Result<String, GnsError> {
    decode_one(result, ParamType::String, what)?
        .into_string()
        .ok_or(GnsError::Decode(what))
}

fn encode_aggregate3(calls: Vec<(Address, Vec<u8>)>) -> Vec<u8> {
    let calls = calls
        .into_iter()
        .map(|(target, data)| {
            Token::Tuple(vec![
                Token::Address(target),
                // allowFailure: a failing read must not take the batch down
                Token::Bool(true),
                Token::Bytes(data),
            ])
        })
        .collect();

    [&AGGREGATE3 as &[u8], &abi::encode(&[Token::Array(calls)])].concat()
}

fn decode_aggregate3(data: &[u8]) -> Option<Vec<CallResult>> {
    let result_type = ParamType::Array(Box::new(ParamType::Tuple(vec![
        ParamType::Bool,
        ParamType::Bytes,
    ])));

    abi::decode(&[result_type], data)
        .ok()?
        .pop()?
        .into_array()?
        .into_iter()
        .map(|result| match result.into_tuple()?.as_slice() {
            [Token::Bool(success), Token::Bytes(data)] => Some(CallResult {
                success: *success,
                data: data.clone(),
            }),
            _ => None,
        })
        .collect()
}

/// Runs `calls` in one `eth_call` and returns the number and timestamp of the
/// block they were all served from, followed by their results.
async fn multicall(
    provider: &Provider<Http>,
    contracts: &GnsContracts,
    calls: Vec<(Address, Vec<u8>)>,
) -> Result<(u64, u64, Vec<CallResult>), GnsError> {
    let expected = calls.len();

    let calls = [
        vec![
            (contracts.multicall, GET_BLOCK_NUMBER.to_vec()),
            (contracts.multicall, GET_CURRENT_BLOCK_TIMESTAMP.to_vec()),
        ],
        calls,
    ]
    .concat();

    let response = call(provider, contracts.multicall, encode_aggregate3(calls)).await?;

    let mut results = decode_aggregate3(&response)
        .filter(|results| results.len() == expected + 2)
        .ok_or(GnsError::Decode("Multicall3.aggregate3"))?;

    let block = decode_u64(&results[0], "Multicall3.getBlockNumber")?;
    let timestamp = decode_u64(&results[1], "Multicall3.getCurrentBlockTimestamp")?;

    Ok((block, timestamp, results.split_off(2)))
}

/// Resolves `lookups` for `name`.
#[instrument(skip(provider, lookups))]
pub async fn resolve_name(
    provider: &Provider<Http>,
    contracts: &GnsContracts,
    name: &GweiName,
    lookups: &[ENSLookup],
) -> Result<NameSnapshot, GnsError> {
    let node = name.node();

    let calls = [
        // `addr(bytes32)` is `NameNFT.resolve`: the explicitly set address or
        // else the owner, and the zero address iff the name is not active.
        (contracts.name_nft, ENSLookup::Addr.calldata(&node)),
        (contracts.name_nft, word(EXPIRES_AT, name.root().as_bytes())),
    ]
    .into_iter()
    .chain(
        lookups
            .iter()
            .map(|lookup| (contracts.name_nft, lookup.calldata(&node))),
    )
    .collect();

    let (block, timestamp, mut results) = multicall(provider, contracts, calls).await?;

    let records = results.split_off(2);

    let resolved = decode_one(&results[0], ParamType::Address, "NameNFT.addr")?
        .into_address()
        .ok_or(GnsError::Decode("NameNFT.addr"))?;

    Ok(NameSnapshot {
        block,
        timestamp,
        expires_at: decode_u64(&results[1], "NameNFT.expiresAt")?,
        records: (!resolved.is_zero()).then_some(records),
    })
}

/// Looks up the primary name of `address`.
#[instrument(skip(provider))]
pub async fn resolve_reverse(
    provider: &Provider<Http>,
    contracts: &GnsContracts,
    address: &Address,
) -> Result<ReverseSnapshot, GnsError> {
    let calls = vec![
        (
            contracts.name_nft,
            word(REVERSE_RESOLVE, address.as_bytes()),
        ),
        (contracts.name_nft, word(PRIMARY_NAME, address.as_bytes())),
    ];

    let (block, _, results) = multicall(provider, contracts, calls).await?;

    let primary = decode_one(&results[1], ParamType::Uint(256), "NameNFT.primaryName")?
        .into_uint()
        .ok_or(GnsError::Decode("NameNFT.primaryName"))?;

    Ok(ReverseSnapshot {
        block,
        name: decode_string(&results[0], "NameNFT.reverseResolve")?,
        primary: (!primary.is_zero()).then(|| node_from_token(primary)),
    })
}

/// `NameNFT.expiresAt`: expiry of a top-level name, 0 for anything else.
pub async fn expires_at(
    provider: &Provider<Http>,
    contracts: &GnsContracts,
    node: &H256,
) -> Result<u64, GnsError> {
    let data = call(
        provider,
        contracts.name_nft,
        word(EXPIRES_AT, node.as_bytes()),
    )
    .await?
    .to_vec();

    decode_u64(
        &CallResult {
            success: true,
            data,
        },
        "NameNFT.expiresAt",
    )
}

/// `NameNFT.getFullName`: the name of a token regardless of expiry, empty if
/// the token never existed or is a stale subdomain.
pub async fn full_name(
    provider: &Provider<Http>,
    contracts: &GnsContracts,
    node: &H256,
) -> Result<String, GnsError> {
    let data = call(
        provider,
        contracts.name_nft,
        word(GET_FULL_NAME, node.as_bytes()),
    )
    .await?
    .to_vec();

    decode_string(
        &CallResult {
            success: true,
            data,
        },
        "NameNFT.getFullName",
    )
}

#[cfg(test)]
mod tests {
    use ethers_core::utils::id;

    use super::*;

    #[test]
    fn test_selectors() {
        assert_eq!(AGGREGATE3, id("aggregate3((address,bool,bytes)[])"));
        assert_eq!(GET_BLOCK_NUMBER, id("getBlockNumber()"));
        assert_eq!(
            GET_CURRENT_BLOCK_TIMESTAMP,
            id("getCurrentBlockTimestamp()")
        );
        assert_eq!(EXPIRES_AT, id("expiresAt(uint256)"));
        assert_eq!(GET_FULL_NAME, id("getFullName(uint256)"));
        assert_eq!(REVERSE_RESOLVE, id("reverseResolve(address)"));
        assert_eq!(PRIMARY_NAME, id("primaryName(address)"));
    }

    #[test]
    fn test_word() {
        let address: Address = "0xBBBCe157eCBb945b1F92Af88413b911910C727c8"
            .parse()
            .unwrap();

        assert_eq!(
            word(REVERSE_RESOLVE, address.as_bytes()),
            hex!("9af8b7aa000000000000000000000000bbbce157ecbb945b1f92af88413b911910c727c8")
        );
    }

    #[test]
    fn test_aggregate3_roundtrip() {
        let target: Address = "0x9D51D507BC7264d4fE8Ad1cf7Fe191933A0a81d6"
            .parse()
            .unwrap();

        let encoded = encode_aggregate3(vec![(target, vec![0xde, 0xad])]);

        assert_eq!(encoded[..4], AGGREGATE3);

        let decoded = abi::decode(
            &[ParamType::Array(Box::new(ParamType::Tuple(vec![
                ParamType::Address,
                ParamType::Bool,
                ParamType::Bytes,
            ])))],
            &encoded[4..],
        )
        .unwrap();

        assert_eq!(
            decoded,
            vec![Token::Array(vec![Token::Tuple(vec![
                Token::Address(target),
                Token::Bool(true),
                Token::Bytes(vec![0xde, 0xad]),
            ])])]
        );

        let response = abi::encode(&[Token::Array(vec![
            Token::Tuple(vec![Token::Bool(true), Token::Bytes(vec![1, 2, 3])]),
            Token::Tuple(vec![Token::Bool(false), Token::Bytes(vec![])]),
        ])]);
        let results = decode_aggregate3(&response).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert_eq!(results[0].data, vec![1, 2, 3]);
        assert!(!results[1].success);

        assert!(decode_aggregate3(&[]).is_none());
    }
}
