use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use tracing::{info, instrument};

use crate::cache::CacheError;
use crate::core::address::{unix_now, AddressResolveError};
use crate::core::error::ProfileError;
use crate::core::gns::{self, GweiName};
use crate::core::lookup_data::LookupInfo;
use crate::core::{ENSService, Profile};
use crate::models::lookup::ENSLookup;
use crate::utils::eip55::EIP55Address;

impl ENSService {
    #[instrument(skip(self))]
    pub async fn resolve_profile(
        &self,
        lookup: LookupInfo,
        fresh: bool,
    ) -> Result<Profile, ProfileError> {
        let gwei_name = match lookup {
            LookupInfo::Name(name) => GweiName::parse(&name)?,
            LookupInfo::Address(address) => self
                .primary_from_address(&address, fresh)
                .await
                .map_err(|error| match error {
                    AddressResolveError::NotFound => ProfileError::NotFound,
                    // not being able to tell is not the same as having no name
                    error => ProfileError::AddressResolveError(error),
                })?,
        };

        // Keyed by namehash, which is what NameNFT events carry
        let cache_key = gns::profile_key(&gwei_name.node());
        let cache_tags = gwei_name.tags();
        let cache_ttl = self.cache_ttl.unwrap_or(600);

        let name = gwei_name.as_str();

        info!(
            name = name,
            cache_key = cache_key,
            fresh = fresh,
            "Looking up profile for {name}..."
        );

        // If the value is in the cache, return it
        if !fresh {
            if let Ok(value) = self.cache.get(&cache_key).await {
                if value.is_empty() {
                    return Err(ProfileError::NotFound);
                }

                let entry_result: Result<Profile, _> = serde_json::from_str(value.as_str());
                if let Ok(entry) = entry_result {
                    // TODO: +1 on cache hit popularity
                    return Ok(entry);
                }
                // TODO: Else, warn about unparsable data in cache
            }
        }

        // Preset Hardcoded Lookups
        let mut calldata: HashSet<ENSLookup> = HashSet::new();

        let (addr_key, avatar_key, header_key, display_key, contenthash_key) = (
            ENSLookup::Addr,
            ENSLookup::StaticImage("avatar"),
            ENSLookup::StaticImage("header"),
            ENSLookup::StaticText("display"),
            ENSLookup::ContentHash,
        );

        calldata.extend([
            addr_key.clone(),
            avatar_key.clone(),
            header_key.clone(),
            display_key.clone(),
            contenthash_key.clone(),
        ]);

        calldata.extend(self.profile_records.iter().cloned().map(ENSLookup::Text));
        calldata.extend(
            self.profile_chains
                .iter()
                .cloned()
                .map(ENSLookup::Multicoin),
        );

        let snapshot = self
            .resolve_snapshot(&gwei_name, &Vec::from_iter(calldata))
            .await?;

        let cache_error =
            |CacheError::Other(err)| ProfileError::Other(format!("cache set failed: {}", err));

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

        let address = resolved.records.get(&addr_key).cloned();
        let avatar = resolved.records.get(&avatar_key).cloned();
        let header = resolved.records.get(&header_key).cloned();
        let display_record = resolved.records.get(&display_key).cloned();

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

        let contenthash = resolved.records.get(&contenthash_key).cloned();

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

        let value = Profile {
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
        };

        // Update metrics
        // TODO: +1 on cache hit popularity discover

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
