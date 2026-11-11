use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use tracing::{info, instrument};

use crate::cache::CacheError;
use crate::core::address::unix_now;
use crate::core::error::ProfileError;
use crate::core::gns::{self, GweiName};
use crate::core::lookup_data::{LookupInfo, Name};
use crate::core::records::ResolvedCalldata;
use crate::core::{ENSService, Profile};
use crate::models::lookup::ENSLookup;
use crate::utils::eip55::EIP55Address;

const ADDR: ENSLookup = ENSLookup::Addr;
const AVATAR: ENSLookup = ENSLookup::StaticImage("avatar");
const HEADER: ENSLookup = ENSLookup::StaticImage("header");
const DISPLAY: ENSLookup = ENSLookup::StaticText("display");
const CONTENTHASH: ENSLookup = ENSLookup::ContentHash;

fn cache_error(CacheError::Other(err): CacheError) -> ProfileError {
    ProfileError::Other(format!("cache set failed: {}", err))
}

impl ENSService {
    /// Resolves the profile of a name, or of the primary name of an address.
    ///
    /// Names ending in `.gwei` are served by the Gwei Name Service and cached
    /// until an on-chain event changes them; every other name is served by
    /// ENS and cached for `cache_ttl`.
    #[instrument(skip(self))]
    pub async fn resolve_profile(
        &self,
        lookup: LookupInfo,
        fresh: bool,
    ) -> Result<Profile, ProfileError> {
        match self.lookup_name(lookup, fresh).await? {
            Name::Ens(name) => self.resolve_ens_profile(&name, fresh).await,
            Name::Gwei(name) => self.resolve_gwei_profile(&name, fresh).await,
        }
    }

    /// Looks a profile up in the cache. An empty entry stands for a name that
    /// is known not to resolve.
    async fn cached_profile(&self, cache_key: &str) -> Option<Result<Profile, ProfileError>> {
        let value = self.cache.get(cache_key).await.ok()?;

        if value.is_empty() {
            return Some(Err(ProfileError::NotFound));
        }

        // TODO: +1 on cache hit popularity
        // TODO: Else, warn about unparsable data in cache
        serde_json::from_str(value.as_str()).ok().map(Ok)
    }

    /// The records a profile is made of.
    fn profile_lookups(&self) -> Vec<ENSLookup> {
        // Preset Hardcoded Lookups
        let mut calldata: HashSet<ENSLookup> = HashSet::new();

        calldata.extend([ADDR, AVATAR, HEADER, DISPLAY, CONTENTHASH]);

        calldata.extend(self.profile_records.iter().cloned().map(ENSLookup::Text));
        calldata.extend(
            self.profile_chains
                .iter()
                .cloned()
                .map(ENSLookup::Multicoin),
        );

        Vec::from_iter(calldata)
    }

    /// Assembles the profile of `name` from its resolved records.
    fn build_profile(&self, name: &str, resolved: ResolvedCalldata) -> Profile {
        let address = resolved.records.get(&ADDR).cloned();
        let avatar = resolved.records.get(&AVATAR).cloned();
        let header = resolved.records.get(&HEADER).cloned();
        let display_record = resolved.records.get(&DISPLAY).cloned();

        let display = match display_record {
            Some(display) if display.to_lowercase() == name.to_lowercase() => display,
            _ => name.to_string(),
        };

        info!(
            name = name,
            address,
            avatar = ?avatar,
            header = ?header,
            "Profile for {name} found"
        );

        let contenthash = resolved.records.get(&CONTENTHASH).cloned();

        let records: BTreeMap<String, String> = self
            .profile_records
            .iter()
            .filter_map(|record| {
                resolved
                    .records
                    .get(&ENSLookup::Text(record.to_string()))
                    .map(|value| (record.to_string(), value.to_string()))
            })
            .collect();

        let chains: BTreeMap<String, String> = self
            .profile_chains
            .iter()
            .filter_map(|coin_type| {
                resolved
                    .records
                    .get(&ENSLookup::Multicoin(coin_type.clone()))
                    .map(|value| (coin_type.to_string(), value.to_string()))
            })
            .collect();

        Profile {
            name: name.to_string(),
            address: address.and_then(|it| EIP55Address::from_str(it.as_str()).ok()),
            avatar,
            header,
            display,
            contenthash,
            records,
            chains,
            fresh: chrono::offset::Utc::now().timestamp_millis(),
            resolver: EIP55Address(resolved.resolver),
            ccip_urls: resolved.ccip_urls,
            errors: resolved
                .invalid
                .iter()
                .map(|(key, value)| (key.name(), value.to_string()))
                .collect(),
        }
    }

    /// The profile of an ENS name, cached for `cache_ttl`.
    async fn resolve_ens_profile(&self, name: &str, fresh: bool) -> Result<Profile, ProfileError> {
        let cache_key = format!("n:{name}");

        info!(
            name = name,
            cache_key = cache_key,
            fresh = fresh,
            "Looking up profile for {name}..."
        );

        // If the value is in the cache, return it
        if !fresh {
            if let Some(cached) = self.cached_profile(&cache_key).await {
                return cached;
            }
        }

        let resolved = self
            .resolve_ens_records(name, &self.profile_lookups())
            .await?;

        let value = self.build_profile(name, resolved);

        // Update metrics
        // TODO: +1 on cache hit popularity discover

        let response =
            serde_json::to_string(&value).map_err(|err| ProfileError::Other(err.to_string()))?;

        self.cache
            .set(&cache_key, &response, self.cache_ttl.unwrap_or(600))
            .await
            .map_err(cache_error)?;

        Ok(value)
    }

    /// The profile of a `.gwei` name, cached until an event of the Gwei Name
    /// Service drops it, the name expires or `cache_ttl` runs out.
    async fn resolve_gwei_profile(
        &self,
        name: &GweiName,
        fresh: bool,
    ) -> Result<Profile, ProfileError> {
        // Keyed by namehash, which is what NameNFT events carry
        let cache_key = gns::profile_key(&name.node());
        let cache_tags = name.tags();
        let cache_ttl = self.cache_ttl.unwrap_or(600);

        info!(
            name = name.as_str(),
            cache_key = cache_key,
            fresh = fresh,
            "Looking up profile for {name}..."
        );

        // If the value is in the cache, return it
        if !fresh {
            if let Some(cached) = self.cached_profile(&cache_key).await {
                return cached;
            }
        }

        let snapshot = self.resolve_snapshot(name, &self.profile_lookups()).await?;

        let Some(resolved) = snapshot.resolved else {
            // Unregistered, expired or stale. Registration, renewal and
            // re-registration of the name or one of its parents all emit
            // events that drop this entry again.
            self.cache
                .set_tagged(&cache_key, "", cache_ttl, &cache_tags, snapshot.block)
                .await
                .map_err(cache_error)?;

            return Err(ProfileError::NotFound);
        };

        let value = self.build_profile(name.as_str(), resolved);

        let response =
            serde_json::to_string(&value).map_err(|err| ProfileError::Other(err.to_string()))?;

        // Expiry emits no event, so the entry must not outlive the name. Use
        // whichever clock is further ahead to stay on the safe side.
        let now = unix_now().max(snapshot.timestamp);

        if let Some(ttl) = gns::capped_ttl(cache_ttl, snapshot.expires_at, now) {
            self.cache
                .set_tagged(&cache_key, &response, ttl, &cache_tags, snapshot.block)
                .await
                .map_err(cache_error)?;
        }

        Ok(value)
    }
}
