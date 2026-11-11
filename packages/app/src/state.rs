use std::env;
use std::sync::Arc;

use std::time::Duration;

use enstate_shared::cache::{CacheLayer, PassthroughCacheLayer};
use enstate_shared::core::gns;
use enstate_shared::core::resolvers::gns::GnsContracts;
use enstate_shared::core::ENSService;
use enstate_shared::discovery::Discovery;
use enstate_shared::models::{
    multicoin::cointype::{coins::CoinType, Coins},
    records::Records,
};
use ethers_core::types::H160;
use tracing::{info, warn};
use url::Url;

use crate::discovery::engine::DiscoveryEngine;
use crate::http::RateLimiter;
use crate::provider::RoundRobin;
use crate::telemetry::metrics::Metrics;
use crate::{cache, database, watcher};

#[allow(clippy::module_name_repetitions)]
pub struct AppState {
    pub service: ENSService,
    pub metrics: Metrics,
    pub rate_limiter: RateLimiter,
    /// Settings of the `NameNFT` log watcher, `None` if it is not to run.
    pub watcher: Option<watcher::Config>,
}

fn env_number(name: &str, default: u64) -> u64 {
    env::var(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name} should be a number"))
    })
}

impl AppState {
    pub async fn new() -> Self {
        let profile_records = env::var("PROFILE_RECORDS").map_or_else(
            |_| Records::default().records,
            |s| s.split(',').map(ToString::to_string).collect(),
        );

        let multicoin_chains: Vec<CoinType> = env::var("MULTICOIN_CHAINS").map_or_else(
            |_| Coins::default().coins,
            |s| {
                let numbers = s
                    .split(',')
                    .filter_map(|it| it.parse::<u64>().ok())
                    .collect::<Vec<_>>();

                numbers.iter().map(|num| CoinType::from(*num)).collect()
            },
        );

        let raw_rpc_urls: String = env::var("RPC_URL").expect("RPC_URL should've been set");
        let rpc_urls = raw_rpc_urls
            .split(',')
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        info!("Connecting to Redis...");

        let redis = database::setup().await.ok();
        let has_cache = redis.is_some();

        let cache = redis.map_or_else(
            || {
                warn!("failed to connect to redis, using no cache");

                Box::new(PassthroughCacheLayer {}) as Box<dyn CacheLayer>
            },
            |redis| {
                info!("Connected to Redis");

                Box::new(cache::Redis::new(redis)) as Box<dyn CacheLayer>
            },
        );

        let provider = RoundRobin::new(rpc_urls);

        let opensea_api_key =
            env::var("OPENSEA_API_KEY").expect("OPENSEA_API_KEY should've been set");

        let ipfs_gateway = env::var("IPFS_GATEWAY").map_or_else(
            |_| "https://ipfs.io/ipfs/".to_string(),
            |ipfs_gateway| Url::parse(&ipfs_gateway).unwrap().to_string(),
        );

        let arweave_gateway = env::var("AR_GATEWAY").map_or_else(
            |_| "https://arweave.net/".to_string(),
            |arweave_gateway| Url::parse(&arweave_gateway).unwrap().to_string(),
        );

        let gns_contract = env::var("GNS_CONTRACT").map_or(gns::NAME_NFT, |address| {
            address
                .parse::<H160>()
                .expect("GNS_CONTRACT should be a valid address")
        });

        let multicall = env::var("MULTICALL_ADDRESS").map_or(gns::MULTICALL3, |address| {
            address
                .parse::<H160>()
                .expect("MULTICALL_ADDRESS should be a valid address")
        });

        let max_bulk_size =
            env::var("MAX_BULK_SIZE").map_or(10, |bulk_size| bulk_size.parse().unwrap());

        let cache_ttl =
            env::var("PROFILE_CACHE_TTL").map_or(Some(600), |cache_ttl| cache_ttl.parse().ok());

        let watcher_enabled =
            env::var("WATCHER_ENABLED").map_or(true, |enabled| enabled != "false");

        // without a cache there is nothing to keep in sync
        let watcher = (watcher_enabled && has_cache).then(|| watcher::Config {
            contract: gns_contract,
            ws_url: env::var("WS_RPC_URL").ok().filter(|url| !url.is_empty()),
            poll_interval: Duration::from_secs(env_number("WATCHER_POLL_INTERVAL", 12).max(1)),
            reorg_depth: env_number("WATCHER_REORG_DEPTH", 12),
            max_block_range: env_number("WATCHER_MAX_BLOCK_RANGE", 1000),
            cache_ttl: cache_ttl.unwrap_or(600).into(),
        });

        if !watcher_enabled {
            warn!("watcher disabled, cache entries are only refreshed when they expire");
        }

        let meilisearch_url = env::var("MEILI_ENDPOINT").ok();
        let meilisearch_key = env::var("MEILI_KEY").ok();

        let discovery = meilisearch_url.map(|url| {
            let engine = DiscoveryEngine::new(&url, meilisearch_key.as_deref());

            Box::new(engine) as Box<dyn Discovery>
        });

        Self {
            rate_limiter: RateLimiter::new(),
            service: ENSService {
                discovery,
                cache,
                rpc: Box::new(provider),
                opensea_api_key,
                ipfs_gateway,
                arweave_gateway,
                max_bulk_size,
                cache_ttl,
                profile_records: Arc::from(profile_records),
                profile_chains: Arc::from(multicoin_chains),
                gns: GnsContracts {
                    name_nft: gns_contract,
                    multicall,
                },
            },
            metrics: Metrics::new(),
            watcher,
        }
    }
}
