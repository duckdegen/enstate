//! Gwei Name Service (GNS) primitives: `.gwei` name parsing, EIP-137 namehashes
//! and the cache key / invalidation tag scheme shared by the resolver and the
//! on-chain event watcher.
//!
//! GNS keeps everything in one contract (`NameNFT`): it is the ERC-721, the
//! registrar and the resolver, and a token id is `uint256(namehash(name))`.
//! Cache entries are therefore keyed by namehash, which is also what every
//! `NameNFT` event carries, so an event maps onto cache keys without any lookup.

use ethers_core::types::{Address, H160, H256, U256};
use ethers_core::utils::keccak256;
use hex_literal::hex;
use thiserror::Error;

/// The only TLD served by this instance.
pub const TLD: &str = "gwei";

/// `namehash("gwei")`, as published by `NameNFT.GWEI_NODE`.
pub const GWEI_NODE: H256 = H256(hex!(
    "cca9c7f2dbe2808af0de2982fc84314bfa68a82a6a60ad5cd757f91a233d7d7f"
));

/// `NameNFT` (ERC-721 + registrar + resolver), same address on mainnet and Sepolia.
pub const NAME_NFT: Address = H160(hex!("9D51D507BC7264d4fE8Ad1cf7Fe191933A0a81d6"));

/// Multicall3, same address on mainnet and Sepolia.
pub const MULTICALL3: Address = H160(hex!("cA11bde05977b3631167028862bE2a173976CA11"));

/// `NameNFT.MAX_LABEL_LENGTH`, in bytes.
const MAX_LABEL_LENGTH: usize = 255;

#[derive(Error, Clone, Debug, PartialEq, Eq)]
pub enum NameError {
    #[error("Unsupported name: only .{TLD} names are served by this instance")]
    UnsupportedTld,

    #[error("Invalid name: labels must be between 1 and {MAX_LABEL_LENGTH} bytes")]
    InvalidLabel,
}

/// A syntactically valid `.gwei` name together with the namehash of the name
/// itself and of each of its parents below the TLD.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GweiName {
    name: String,
    /// `nodes[0]` is the name itself, the last entry its top-level
    /// (`label.gwei`) ancestor. Never empty.
    nodes: Vec<H256>,
}

impl GweiName {
    /// Parses `sub.name.gwei`. Mirrors `NameNFT.computeNamehash`: ASCII letters
    /// are lower-cased, every other byte is hashed as-is (callers are expected
    /// to have normalized Unicode beforehand). Unlike the contract a bare label
    /// without the `.gwei` suffix is rejected, so that `foo.eth` can never be
    /// mistaken for a GNS name.
    pub fn parse(input: &str) -> Result<Self, NameError> {
        let name = input.to_ascii_lowercase();

        let labels = name
            .strip_suffix(TLD)
            .and_then(|rest| rest.strip_suffix('.'))
            .ok_or(NameError::UnsupportedTld)?;

        let mut nodes = Vec::new();
        let mut node = GWEI_NODE;

        for label in labels.rsplit('.') {
            if label.is_empty() || label.len() > MAX_LABEL_LENGTH {
                return Err(NameError::InvalidLabel);
            }

            node = H256(keccak256(
                [node.as_bytes(), &keccak256(label.as_bytes())].concat(),
            ));
            nodes.push(node);
        }

        nodes.reverse();

        Ok(Self { name, nodes })
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Namehash of the name, i.e. its `NameNFT` token id.
    pub fn node(&self) -> H256 {
        self.nodes[0]
    }

    /// Namehash of the top-level (`label.gwei`) name this name lives under.
    /// It is the only node in the chain that carries an expiry.
    pub fn root(&self) -> H256 {
        self.nodes[self.nodes.len() - 1]
    }

    /// Namehashes of the parents of this name, nearest first.
    pub fn ancestors(&self) -> &[H256] {
        &self.nodes[1..]
    }

    /// Tags a cache entry derived from this name has to carry: the name's own
    /// token tag, and the subtree tag of every parent (a subdomain is only
    /// active while its whole parent chain is).
    pub fn tags(&self) -> Vec<String> {
        std::iter::once(token_tag(&self.node()))
            .chain(self.ancestors().iter().map(subtree_tag))
            .collect()
    }
}

impl std::fmt::Display for GweiName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// Converts a `NameNFT` token id into the namehash it is.
pub fn node_from_token(token: U256) -> H256 {
    let mut bytes = [0u8; 32];
    token.to_big_endian(&mut bytes);
    H256(bytes)
}

/// Cache key of the profile of the name with the given namehash.
pub fn profile_key(node: &H256) -> String {
    format!("gwei:n:{node:?}")
}

/// Cache key of the primary name of an address.
pub fn reverse_key(address: &Address) -> String {
    format!("gwei:a:{address:?}")
}

/// Tag of every cache entry that depends on the records, owner or activity of
/// exactly this name: its profile, and the primary name of every address whose
/// on-chain `primaryName` is this token.
pub fn token_tag(node: &H256) -> String {
    format!("gwei:t:{node:?}")
}

/// Tag of every cache entry that depends on a name *below* this one.
pub fn subtree_tag(node: &H256) -> String {
    format!("gwei:s:{node:?}")
}

/// Caps a cache TTL so that an entry never outlives the name it was derived
/// from: `NameNFT` treats a name as active only while
/// `block.timestamp <= expiresAt`, and expiry emits no event.
///
/// Returns `None` when the name is already expired at `now` (nothing may be
/// cached).
pub fn capped_ttl(ttl: u32, expires_at: u64, now: u64) -> Option<u32> {
    let remaining = expires_at.checked_sub(now).filter(|it| *it > 0)?;

    Some(u32::try_from(remaining).map_or(ttl, |remaining| remaining.min(ttl)))
}

#[cfg(test)]
mod tests {
    use ethers::providers::namehash;

    use super::*;

    #[test]
    fn test_tld_node() {
        assert_eq!(namehash(TLD), GWEI_NODE);
    }

    #[test]
    fn test_parse_top_level() {
        let name = GweiName::parse("gns.gwei").unwrap();

        assert_eq!(name.as_str(), "gns.gwei");
        assert_eq!(name.node(), namehash("gns.gwei"));
        assert_eq!(name.root(), name.node());
        assert!(name.ancestors().is_empty());
        // NameNFT.computeId("gns.gwei") on mainnet
        assert_eq!(
            name.node(),
            H256(hex!(
                "41593da8bfd4bccd6b82a0f4edd9adacf5a92212c6238c69b4704fd763bcc162"
            ))
        );
    }

    #[test]
    fn test_parse_subdomain() {
        let name = GweiName::parse("a.b.c.gwei").unwrap();

        assert_eq!(name.node(), namehash("a.b.c.gwei"));
        // NameNFT.computeId("a.b.c.gwei") on mainnet
        assert_eq!(
            name.node(),
            H256(hex!(
                "e8ecd4ddb31a1590381a227f52e90899015af42fb8a1cdbc140c0463d603334d"
            ))
        );
        assert_eq!(
            name.ancestors(),
            &[namehash("b.c.gwei"), namehash("c.gwei")]
        );
        assert_eq!(name.root(), namehash("c.gwei"));
        assert_eq!(
            name.tags(),
            vec![
                token_tag(&namehash("a.b.c.gwei")),
                subtree_tag(&namehash("b.c.gwei")),
                subtree_tag(&namehash("c.gwei")),
            ]
        );
    }

    #[test]
    fn test_parse_lowercases_ascii_only() {
        assert_eq!(
            GweiName::parse("GnS.GWEI").unwrap(),
            GweiName::parse("gns.gwei").unwrap()
        );

        // non-ASCII bytes are hashed untouched, like NameNFT._toLowerSlice
        let name = GweiName::parse("É.gwei").unwrap();
        assert_eq!(name.as_str(), "É.gwei");
        assert_ne!(name.node(), GweiName::parse("é.gwei").unwrap().node());
    }

    #[test]
    fn test_parse_rejects_other_names() {
        for name in ["luc.eth", "gwei", "foo", "foo.gwei.eth", "foogwei", ""] {
            assert_eq!(
                GweiName::parse(name),
                Err(NameError::UnsupportedTld),
                "{name}"
            );
        }

        for name in [".gwei", "a..gwei", ".a.gwei", "a..b.gwei"] {
            assert_eq!(
                GweiName::parse(name),
                Err(NameError::InvalidLabel),
                "{name}"
            );
        }

        let long = format!("{}.gwei", "a".repeat(256));
        assert_eq!(GweiName::parse(&long), Err(NameError::InvalidLabel));
        assert!(GweiName::parse(&long[1..]).is_ok());
    }

    #[test]
    fn test_node_from_token() {
        let node = namehash("gns.gwei");

        assert_eq!(
            node_from_token(U256::from_big_endian(node.as_bytes())),
            node
        );
    }

    #[test]
    fn test_keys() {
        let node = namehash("gns.gwei");
        let address: Address = "0xBBBCe157eCBb945b1F92Af88413b911910C727c8"
            .parse()
            .unwrap();

        assert_eq!(
            profile_key(&node),
            format!("gwei:n:0x{}", hex::encode(node))
        );
        assert_eq!(token_tag(&node), format!("gwei:t:0x{}", hex::encode(node)));
        assert_eq!(
            subtree_tag(&node),
            format!("gwei:s:0x{}", hex::encode(node))
        );
        assert_eq!(
            reverse_key(&address),
            "gwei:a:0xbbbce157ecbb945b1f92af88413b911910c727c8"
        );
    }

    #[test]
    fn test_capped_ttl() {
        // far from expiry: the configured TTL applies
        assert_eq!(capped_ttl(600, 1_000_000, 1_000), Some(600));
        // close to expiry: the entry dies with the name
        assert_eq!(capped_ttl(600, 1_100, 1_000), Some(100));
        assert_eq!(capped_ttl(600, 1_001, 1_000), Some(1));
        // expired (a name is active while timestamp <= expiresAt, but a
        // zero-second entry cannot be stored)
        assert_eq!(capped_ttl(600, 1_000, 1_000), None);
        assert_eq!(capped_ttl(600, 999, 1_000), None);
        // subdomains report expiresAt = 0 themselves: never cache on that
        assert_eq!(capped_ttl(600, 0, 1_000), None);
        // remaining lifetime beyond u32 does not overflow
        assert_eq!(capped_ttl(600, u64::MAX, 0), Some(600));
    }
}
