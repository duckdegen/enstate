use ethers::middleware::MiddlewareBuilder;
use ethers::providers::ProviderError;
use ethers_ccip_read::CCIPReadMiddleware;
use ethers_core::types::Address;
use thiserror::Error;
use tracing::instrument;

use crate::core::gns::{self, GweiName};
use crate::core::lookup_data::{Name, NameSystem};
use crate::core::resolvers::gns::{self as gns_resolver, expires_at, full_name, GnsError};
use crate::core::resolvers::reverse::{resolve_reverse, ReverseResolveError};
use crate::core::ENSService;

#[derive(Error, Debug)]
pub enum AddressResolveError {
    #[error("Primary name not found")]
    NotFound,

    #[error("Cache operation failed: {0}")]
    CacheFail(&'static str),

    #[error("RPC error: {0}")]
    RPCError(#[from] ProviderError),

    #[error("Reverse resolution error: {0}")]
    ReverseResolutionError(#[from] ReverseResolveError),

    #[error("Reverse resolution error: {0}")]
    GnsError(#[from] GnsError),
}

/// Picks the primary name of an address from what each name system answered:
/// the name of the first system in `order` that has one.
///
/// A system that could not be asked only matters if no system preferred over
/// it has a name: not being able to tell is not the same as having no name.
fn pick_primary(
    order: &[NameSystem],
    ens: Result<Option<String>, AddressResolveError>,
    gwei: Result<Option<GweiName>, AddressResolveError>,
) -> Result<Name, AddressResolveError> {
    let (mut ens, mut gwei) = (Some(ens), Some(gwei));

    for system in order {
        let name = match system {
            NameSystem::Ens => ens.take().transpose()?.flatten().map(Name::Ens),
            NameSystem::Gwei => gwei.take().transpose()?.flatten().map(Name::Gwei),
        };

        if let Some(name) = name {
            return Ok(name);
        }
    }

    Err(AddressResolveError::NotFound)
}

impl ENSService {
    /// Returns the primary name of `address`.
    ///
    /// Both name systems are asked at once. If the address has a primary name
    /// in more than one, `name_services_order` decides. Each system keeps its
    /// own cached answer under its own rules, so what one of them remembers
    /// never hides a change in the other.
    #[instrument(skip(self))]
    pub async fn primary_from_address(
        &self,
        address: &Address,
        fresh: bool,
    ) -> Result<Name, AddressResolveError> {
        let (ens, gwei) = futures::join!(
            self.ens_primary(address, fresh),
            self.gwei_primary(address, fresh),
        );

        pick_primary(&self.name_services_order, ens, gwei)
    }

    /// The ENS primary name of `address`: the name its reverse record claims,
    /// provided that name resolves back to the address.
    async fn ens_primary(
        &self,
        address: &Address,
        fresh: bool,
    ) -> Result<Option<String>, AddressResolveError> {
        let cache_key = format!("a:{address:?}");

        let rpc = self.rpc.get_instance();
        let rpc = rpc.wrap_into(CCIPReadMiddleware::new);

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
            let result =
                match resolve_reverse(&rpc, address, &self.universal_resolver).await {
                    Ok(name) => name,
                    // address doesn't resolve, cache ""
                    Err(ReverseResolveError::MissingPrimaryName) => String::new(),
                    // the claimed name does not check out, or the node rejected
                    // the call: no primary name, don't cache
                    Err(
                        ReverseResolveError::AddressMismatch
                        | ReverseResolveError::AddressLookupError(_)
                        | ReverseResolveError::AbiDecodeError(_),
                    ) => return Ok(None),
                    Err(ReverseResolveError::RPCError(ProviderError::JsonRpcClientError(
                        error,
                    ))) if error.as_error_response().is_some() => return Ok(None),
                    // yield error up, don't cache
                    Err(error) => return Err(error.into()),
                };

            // Cache the value, and expire it after 10 minutes
            self.cache
                .set(&cache_key, &result, self.cache_ttl.unwrap_or(600))
                .await
                .map_err(|_| AddressResolveError::CacheFail("set"))?;

            result
        };

        Ok((!name.is_empty()).then_some(name))
    }

    /// The primary `.gwei` name of `address`, i.e. what
    /// `NameNFT.reverseResolve` answers: the name the address picked, provided
    /// it is active and resolves back to the address.
    async fn gwei_primary(
        &self,
        address: &Address,
        fresh: bool,
    ) -> Result<Option<GweiName>, AddressResolveError> {
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

            let reverse = gns_resolver::resolve_reverse(&rpc, &self.gns, address).await?;

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

        Ok(GweiName::parse(&name).ok())
    }
}

/// Seconds since the unix epoch.
pub(crate) fn unix_now() -> u64 {
    u64::try_from(chrono::offset::Utc::now().timestamp()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    use NameSystem::{Ens, Gwei};

    fn ens(name: &str) -> Result<Option<String>, AddressResolveError> {
        Ok(Some(name.to_string()))
    }

    fn gwei(name: &str) -> Result<Option<GweiName>, AddressResolveError> {
        Ok(Some(GweiName::parse(name).unwrap()))
    }

    fn failed<T>() -> Result<Option<T>, AddressResolveError> {
        Err(AddressResolveError::CacheFail("get"))
    }

    fn picked(result: Result<Name, AddressResolveError>) -> String {
        result.unwrap().as_str().to_string()
    }

    #[test]
    fn test_pick_primary_follows_the_order() {
        // an address with a primary name in both systems
        assert_eq!(
            picked(pick_primary(&[Gwei, Ens], ens("a.eth"), gwei("a.gwei"))),
            "a.gwei"
        );
        assert_eq!(
            picked(pick_primary(&[Ens, Gwei], ens("a.eth"), gwei("a.gwei"))),
            "a.eth"
        );

        // a name in one system only is found whatever the order
        for order in [[Gwei, Ens], [Ens, Gwei]] {
            assert_eq!(
                picked(pick_primary(&order, ens("a.eth"), Ok(None))),
                "a.eth"
            );
            assert_eq!(
                picked(pick_primary(&order, Ok(None), gwei("a.gwei"))),
                "a.gwei"
            );
            assert!(matches!(
                pick_primary(&order, Ok(None), Ok(None)),
                Err(AddressResolveError::NotFound)
            ));
        }
    }

    #[test]
    fn test_pick_primary_with_a_failed_lookup() {
        // the preferred system has a name: the other one does not matter
        assert_eq!(
            picked(pick_primary(&[Gwei, Ens], failed(), gwei("a.gwei"))),
            "a.gwei"
        );
        assert_eq!(
            picked(pick_primary(&[Ens, Gwei], ens("a.eth"), failed())),
            "a.eth"
        );

        // the preferred system could not be asked: falling back to the other
        // one could name the address differently than it should be
        assert!(matches!(
            pick_primary(&[Gwei, Ens], ens("a.eth"), failed()),
            Err(AddressResolveError::CacheFail(_))
        ));

        // nothing preferred has a name and the last one could not be asked:
        // that is not "no name"
        assert!(matches!(
            pick_primary(&[Gwei, Ens], failed(), Ok(None)),
            Err(AddressResolveError::CacheFail(_))
        ));
    }
}
