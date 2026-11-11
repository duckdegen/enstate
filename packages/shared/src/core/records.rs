use std::collections::HashMap;
use std::sync::Arc;

use ethers::middleware::MiddlewareBuilder;
use ethers::prelude::Address;
use ethers_ccip_read::CCIPReadMiddleware;
use tracing::instrument;

use crate::core::error::ProfileError;
use crate::core::gns::GweiName;
use crate::core::lookup_data::LookupInfo;
use crate::core::resolvers::gns::resolve_name;
use crate::core::ENSService;
use crate::models::lookup::{ENSLookup, ENSLookupError, LookupState};

pub struct ResolvedCalldata {
    pub resolver: Address,
    pub ccip_urls: Vec<String>,
    pub records: HashMap<ENSLookup, String>,
    pub invalid: HashMap<ENSLookup, ENSLookupError>,
}

/// Records of a name as of one block.
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
    /// Reads and decodes `calldata` for `name` in a single round trip.
    #[instrument(skip_all)]
    pub async fn resolve_snapshot(
        &self,
        name: &GweiName,
        calldata: &[ENSLookup],
    ) -> Result<ResolvedSnapshot, ProfileError> {
        let rpc = self.rpc.get_instance();

        let snapshot = resolve_name(&rpc, &self.gns, name, calldata).await?;

        let Some(data) = snapshot.records else {
            return Ok(ResolvedSnapshot {
                block: snapshot.block,
                timestamp: snapshot.timestamp,
                expires_at: snapshot.expires_at,
                resolved: None,
            });
        };

        let mut results: HashMap<ENSLookup, String> = HashMap::new();
        let mut errors: HashMap<ENSLookup, ENSLookupError> = HashMap::default();

        let lookup_state = LookupState {
            rpc: Arc::new(rpc.wrap_into(CCIPReadMiddleware::new)),
            opensea_api_key: self.opensea_api_key.clone(),
            ipfs_gateway: self.ipfs_gateway.clone(),
            arweave_gateway: self.arweave_gateway.clone(),
        };

        // Results & calldata have the same length
        // Look through all calldata and decode the results at the same index
        for (calldata, res) in calldata.iter().zip(&data) {
            // TODO: think about this
            //  current behaviour ignores all errors from a resolver
            let result = if res.success {
                calldata.decode(&res.data, &lookup_state).await
            } else {
                Ok(String::new())
            };

            match result {
                Ok(result) if !result.is_empty() => {
                    results.insert(calldata.clone(), result);
                }
                Err(error) if !matches!(error, ENSLookupError::CCIPError { .. }) => {
                    errors.insert(calldata.clone(), error);
                }
                _ => {}
            }
        }

        Ok(ResolvedSnapshot {
            block: snapshot.block,
            timestamp: snapshot.timestamp,
            expires_at: snapshot.expires_at,
            resolved: Some(ResolvedCalldata {
                // NameNFT is the resolver of every .gwei name
                resolver: self.gns.name_nft,
                // GNS is fully on-chain
                ccip_urls: Vec::new(),
                records: results,
                invalid: errors,
            }),
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
        let name = match lookup {
            LookupInfo::Name(name) => GweiName::parse(&name)?,
            LookupInfo::Address(address) => self.primary_from_address(&address, fresh).await?,
        };

        self.resolve_snapshot(&name, calldata)
            .await?
            .resolved
            .ok_or(ProfileError::NotFound)
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
