use std::collections::HashMap;
use std::sync::Arc;

use ethers::middleware::MiddlewareBuilder;
use ethers::prelude::Address;
use ethers_ccip_read::CCIPReadMiddleware;
use tracing::instrument;

use crate::core::address::AddressResolveError;
use crate::core::error::ProfileError;
use crate::core::gns::GweiName;
use crate::core::lookup_data::{LookupInfo, Name};
use crate::core::resolvers::gns::resolve_name;
use crate::core::{CCIPProvider, ENSService};
use crate::models::lookup::{ENSLookup, ENSLookupError, LookupState};

use super::resolvers::universal::resolve_universal;

pub struct ResolvedCalldata {
    pub resolver: Address,
    pub ccip_urls: Vec<String>,
    pub records: HashMap<ENSLookup, String>,
    pub invalid: HashMap<ENSLookup, ENSLookupError>,
}

/// Records of a `.gwei` name as of one block.
pub struct ResolvedSnapshot {
    /// Block the records were read at.
    pub block: u64,
    /// Timestamp of that block.
    pub timestamp: u64,
    /// Expiry of the top-level name the name lives under.
    pub expires_at: u64,
    /// `None` if the name does not currently resolve.
    pub resolved: Option<ResolvedCalldata>,
}

impl ENSService {
    /// Turns a lookup into the name it is about: the name itself, or the
    /// primary name of the address.
    pub async fn lookup_name(&self, lookup: LookupInfo, fresh: bool) -> Result<Name, ProfileError> {
        match lookup {
            LookupInfo::Name(name) => Ok(Name::parse(&name)?),
            LookupInfo::Address(address) => self
                .primary_from_address(&address, fresh)
                .await
                .map_err(|error| match error {
                    AddressResolveError::NotFound => ProfileError::NotFound,
                    // not being able to tell is not the same as having no name
                    error => ProfileError::AddressResolveError(error),
                }),
        }
    }

    /// Decodes the raw `results` of `calldata`, which are in the same order.
    async fn decode_records(
        &self,
        rpc: Arc<CCIPProvider>,
        calldata: &[ENSLookup],
        results: Vec<(bool, &[u8])>,
    ) -> (
        HashMap<ENSLookup, String>,
        HashMap<ENSLookup, ENSLookupError>,
    ) {
        let mut records: HashMap<ENSLookup, String> = HashMap::new();
        let mut errors: HashMap<ENSLookup, ENSLookupError> = HashMap::default();

        let lookup_state = LookupState {
            rpc,
            opensea_api_key: self.opensea_api_key.clone(),
            ipfs_gateway: self.ipfs_gateway.clone(),
            arweave_gateway: self.arweave_gateway.clone(),
        };

        // Assume results & calldata have the same length
        // Look through all calldata and decode the results at the same index
        for (calldata, (success, data)) in calldata.iter().zip(results) {
            // TODO: think about this
            //  current behaviour ignores all errors from a resolver
            let result = if success {
                calldata.decode(data, &lookup_state).await
            } else {
                Ok(String::new())
            };

            match result {
                Ok(result) if !result.is_empty() => {
                    records.insert(calldata.clone(), result);
                }
                Err(error) if !matches!(error, ENSLookupError::CCIPError { .. }) => {
                    errors.insert(calldata.clone(), error);
                }
                _ => {}
            }
        }

        (records, errors)
    }

    /// Reads and decodes `calldata` for a `.gwei` name in a single round trip
    /// to `NameNFT`.
    #[instrument(skip_all)]
    pub async fn resolve_snapshot(
        &self,
        name: &GweiName,
        calldata: &[ENSLookup],
    ) -> Result<ResolvedSnapshot, ProfileError> {
        let rpc = self.rpc.get_instance();

        let snapshot = resolve_name(&rpc, &self.gns, name, calldata).await?;

        let resolved = match &snapshot.records {
            Some(data) => {
                let (records, invalid) = self
                    .decode_records(
                        Arc::new(rpc.wrap_into(CCIPReadMiddleware::new)),
                        calldata,
                        data.iter()
                            .map(|it| (it.success, it.data.as_slice()))
                            .collect(),
                    )
                    .await;

                Some(ResolvedCalldata {
                    // NameNFT is the resolver of every .gwei name
                    resolver: self.gns.name_nft,
                    // GNS is fully on-chain
                    ccip_urls: Vec::new(),
                    records,
                    invalid,
                })
            }
            None => None,
        };

        Ok(ResolvedSnapshot {
            block: snapshot.block,
            timestamp: snapshot.timestamp,
            expires_at: snapshot.expires_at,
            resolved,
        })
    }

    /// Reads and decodes `calldata` for an ENS name through the universal
    /// resolver.
    #[instrument(skip_all)]
    pub async fn resolve_ens_records(
        &self,
        name: &str,
        calldata: &[ENSLookup],
    ) -> Result<ResolvedCalldata, ProfileError> {
        let rpc = self.rpc.get_instance();
        let rpc = rpc.wrap_into(CCIPReadMiddleware::new);

        let rpc = Arc::new(rpc);

        // ENS CCIP unwrapper is limited to 50 sub-requests, i.e. per request
        let mut resolves = Vec::new();

        // TODO: make 50 chunk size configurable later
        for chunk in calldata.chunks(50) {
            resolves.push(resolve_universal(name, chunk, &rpc, &self.universal_resolver).await?);
        }

        let Some((_, resolver, ccip_urls)) = resolves.first() else {
            return Err(ProfileError::ImplementationError(String::new()));
        };

        let mut data = Vec::new();

        for (results, _, _) in &resolves {
            for result in results {
                data.push((result.success, result.data.as_slice()));
            }
        }

        let (records, invalid) = self.decode_records(rpc.clone(), calldata, data).await;

        Ok(ResolvedCalldata {
            resolver: *resolver,
            ccip_urls: ccip_urls.clone(),
            records,
            invalid,
        })
    }

    // TODO: per-record caching
    #[instrument(skip_all)]
    pub async fn resolve_records(
        &self,
        lookup: LookupInfo,
        calldata: &[ENSLookup],
        fresh: bool,
    ) -> Result<ResolvedCalldata, ProfileError> {
        match self.lookup_name(lookup, fresh).await? {
            Name::Ens(name) => self.resolve_ens_records(&name, calldata).await,
            Name::Gwei(name) => self
                .resolve_snapshot(&name, calldata)
                .await?
                .resolved
                .ok_or(ProfileError::NotFound),
        }
    }

    // utility function
    #[instrument(skip(self))]
    pub async fn resolve_record_simple(
        &self,
        lookup: LookupInfo,
        record: ENSLookup,
        fresh: bool,
    ) -> Result<String, ProfileError> {
        let resolved = self
            .resolve_records(lookup, std::slice::from_ref(&record), fresh)
            .await?;

        let record = resolved
            .records
            .get(&record)
            .ok_or_else(|| ProfileError::NotFound)?;

        Ok(record.to_string())
    }
}
