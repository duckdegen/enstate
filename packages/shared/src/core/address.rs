use ethers_core::types::Address;
use thiserror::Error;
use tracing::instrument;

use crate::core::gns::{self, GweiName};
use crate::core::resolvers::gns::{expires_at, full_name, resolve_reverse, GnsError};
use crate::core::ENSService;

#[derive(Error, Debug)]
pub enum AddressResolveError {
    #[error("Primary name not found")]
    NotFound,

    #[error("Cache operation failed: {0}")]
    CacheFail(&'static str),

    #[error("Reverse resolution error: {0}")]
    ReverseResolutionError(#[from] GnsError),
}

impl ENSService {
    /// Returns the primary `.gwei` name of `address`, i.e. what
    /// `NameNFT.reverseResolve` answers: the name the address picked, provided
    /// it is active and resolves back to the address.
    #[instrument(skip(self))]
    pub async fn primary_from_address(
        &self,
        address: &Address,
        fresh: bool,
    ) -> Result<GweiName, AddressResolveError> {
        let cache_key = gns::reverse_key(address);

        // TODO: improve
        let cached_name = if fresh {
            None
        } else {
            self.cache.get(&cache_key).await.ok()
        };

        // Get value from the cache otherwise compute
        let name = if let Some(name) = cached_name {
            name
        } else {
            let rpc = self.rpc.get_instance();

            let reverse = resolve_reverse(&rpc, &self.gns, address).await?;

            let ttl = self.cache_ttl.unwrap_or(600);

            // Which names the answer depends on, and for how long it may live.
            // Addresses that never picked a primary name depend on no name at
            // all: only a `PrimaryNameSet` for the address can change that.
            let (tags, ttl) = match (reverse.name.is_empty(), reverse.primary) {
                (true, None) => (Vec::new(), Some(ttl)),
                // The address points at a token that does not (currently)
                // resolve back to it. That changes with the token or one of
                // its parents, so find out which name it is.
                (true, Some(primary)) => {
                    let tags = GweiName::parse(&full_name(&rpc, &self.gns, &primary).await?)
                        .ok()
                        .filter(|name| name.node() == primary)
                        .map_or_else(|| vec![gns::token_tag(&primary)], |name| name.tags());

                    (tags, Some(ttl))
                }
                (false, _) => {
                    let name = GweiName::parse(&reverse.name)
                        .ok()
                        .filter(|name| Some(name.node()) == reverse.primary)
                        .ok_or(GnsError::Decode("NameNFT.reverseResolve"))?;

                    // expiry emits no event: the entry must die with the name
                    let expires_at = expires_at(&rpc, &self.gns, &name.root()).await?;

                    (name.tags(), gns::capped_ttl(ttl, expires_at, unix_now()))
                }
            };

            if let Some(ttl) = ttl {
                self.cache
                    .set_tagged(&cache_key, &reverse.name, ttl, &tags, reverse.block)
                    .await
                    .map_err(|_| AddressResolveError::CacheFail("set"))?;
            }

            reverse.name
        };

        if name.is_empty() {
            return Err(AddressResolveError::NotFound);
        }

        GweiName::parse(&name).map_err(|_| AddressResolveError::NotFound)
    }
}

/// Seconds since the unix epoch.
pub(crate) fn unix_now() -> u64 {
    u64::try_from(chrono::offset::Utc::now().timestamp()).unwrap_or_default()
}
