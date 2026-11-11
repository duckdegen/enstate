//! Keeps the cache in sync with the Gwei Name Service by following the logs
//! of the `NameNFT` contract.
//!
//! Every `NameNFT` event that changes what a name or an address resolves to is
//! turned into the set of cache entries it makes stale (see [`events`]), which
//! are dropped right away; the next request repopulates them from the chain.
//!
//! Logs are collected by polling `eth_getLogs`. Each round re-reads the last
//! `reorg_depth` blocks and compares them with what was applied before, so
//! logs that show up late are picked up and logs that were reorganized away
//! invalidate once more. If a WebSocket endpoint is configured, a log
//! subscription delivers the same logs as soon as they are mined; polling
//! keeps running underneath and covers everything the subscription misses
//! (connection loss, restarts).
//!
//! State changes that emit no event are covered elsewhere: entries never
//! outlive the expiry of the name they belong to, and entries of subdomains
//! are tagged with every parent so they go when a parent is registered again.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use ethers::providers::{Http, Middleware, Provider, ProviderError, Ws};
use ethers::types::{Address, Filter, Log, H256, U256};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{timeout, MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::cache::FENCE_KEY;
use crate::state::AppState;

pub mod events;

use events::{GnsEvent, Invalidation};

/// Last block whose logs have been applied. Lets a restarted instance pick up
/// where it left off.
const CURSOR_KEY: &str = "gwei:cursor";
/// The cursor is kept for a week, or for as long as cache entries live if
/// that is longer.
const CURSOR_TTL: u64 = 7 * 24 * 60 * 60;

/// Ethereum produces at most one block per slot.
const SLOT_SECONDS: u64 = 12;

/// How long a node gets to answer a request.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait before trying to re-establish the log subscription.
const WS_RETRY: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Config {
    /// The `NameNFT` contract.
    pub contract: Address,
    /// Optional WebSocket RPC endpoint for a live log subscription.
    pub ws_url: Option<String>,
    /// How often to poll for logs.
    pub poll_interval: Duration,
    /// How many blocks back from the head are re-read on every poll.
    pub reorg_depth: u64,
    /// Largest block range to request from `eth_getLogs` at once.
    pub max_block_range: u64,
    /// Longest time a cache entry may live, in seconds.
    pub cache_ttl: u64,
}

/// Identifies a log on one particular fork.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LogId {
    block_hash: H256,
    log_index: U256,
}

/// Applied logs by fork, with the block they were mined in and what they
/// invalidated.
type Seen = HashMap<LogId, (u64, Invalidation)>;

/// Invalidations collected from one or more logs.
#[derive(Debug, Default)]
struct Batch {
    invalidation: Invalidation,
    /// Values read at an earlier block than this may be stale.
    block: u64,
}

impl Batch {
    /// A log was mined in `block`: what was read before may be stale.
    fn mined(&mut self, invalidation: &Invalidation, block: u64) {
        self.invalidation.extend(invalidation);
        self.block = self.block.max(block);
    }

    /// A log from `block` is no longer part of the chain: what was read while
    /// it was may be stale, and that includes reads at the current head.
    fn reorged(&mut self, invalidation: &Invalidation, block: u64) {
        self.mined(invalidation, block.saturating_add(1));
    }
}

/// First block to read logs from when starting at `head`.
///
/// A cache entry lives for at most `cache_ttl` seconds, so only events from
/// that long ago can concern an entry that still exists; there is never a
/// reason to go back further, however long the watcher was down.
fn start_block(persisted: Option<u64>, head: u64, cache_ttl: u64) -> u64 {
    let floor = head.saturating_sub(cache_ttl / SLOT_SECONDS + 1);

    persisted
        .map_or(floor, |cursor| cursor.max(floor))
        .min(head)
}

/// What polling `eth_getLogs` for a range of blocks turned up.
#[derive(Debug, Default)]
struct Reconciled {
    /// Invalidations for the logs that are new, and for the previously
    /// applied logs that are gone.
    batch: Batch,
    /// The logs now known for the range.
    current: Seen,
    /// Events that were not seen before, with the block they were mined in.
    fresh: Vec<(u64, GnsEvent)>,
}

/// Compares what `eth_getLogs` returned for `from..=to` with the logs that
/// were applied before.
fn reconcile(seen: &Seen, logs: &[Log], from: u64, to: u64) -> Reconciled {
    let mut batch = Batch::default();
    let mut current = Seen::new();
    let mut fresh = Vec::new();

    for log in logs {
        let Some((id, block, event)) = identify(log) else {
            continue;
        };

        let invalidation = event.invalidation();

        if !seen.contains_key(&id) {
            batch.mined(&invalidation, block);
            fresh.push((block, event));
        }

        current.insert(id, (block, invalidation));
    }

    for (id, (block, invalidation)) in seen {
        if (from..=to).contains(block) && !current.contains_key(id) {
            batch.reorged(invalidation, to);
        }
    }

    Reconciled {
        batch,
        current,
        fresh,
    }
}

/// Decodes a log into the event it carries, where and on which fork.
fn identify(log: &Log) -> Option<(LogId, u64, GnsEvent)> {
    let id = LogId {
        block_hash: log.block_hash?,
        log_index: log.log_index?,
    };

    Some((id, log.block_number?.as_u64(), GnsEvent::decode(log)?))
}

/// Whether the node answered with an error, as opposed to not answering.
fn is_rpc_error(error: &ProviderError) -> bool {
    matches!(error, ProviderError::JsonRpcClientError(error) if error.as_error_response().is_some())
}

struct Watcher {
    state: Arc<AppState>,
    config: Config,
    /// Logs applied so far that are still within reorg depth.
    seen: Seen,
    /// Last block whose logs have been applied.
    cursor: Option<u64>,
}

impl Watcher {
    /// Drops the cache entries a batch of logs made stale.
    async fn apply(&self, batch: &Batch) -> anyhow::Result<()> {
        if batch.invalidation.is_empty() {
            return Ok(());
        }

        let tags = batch.invalidation.tags.iter().cloned().collect::<Vec<_>>();
        let keys = batch.invalidation.keys.iter().cloned().collect::<Vec<_>>();

        let removed = self
            .state
            .service
            .cache
            .invalidate(&tags, &keys, batch.block)
            .await
            .map_err(|error| anyhow::anyhow!("cache invalidation failed: {error:?}"))?;

        self.state.metrics.watcher_invalidated_total.inc_by(removed);

        info!(
            tags = tags.len(),
            keys = keys.len(),
            block = batch.block,
            "Invalidated {removed} cache entries"
        );

        Ok(())
    }

    fn record(&self, event: &GnsEvent, block: u64, removed: bool) {
        self.state
            .metrics
            .watcher_events_total
            .with_label_values(&[event.name()])
            .inc();

        info!(block, removed, "GNS event: {event:?}");
    }

    async fn fetch_logs(
        &self,
        provider: &Provider<Http>,
        from: u64,
        to: u64,
    ) -> anyhow::Result<Vec<Log>> {
        let mut logs = Vec::new();
        let mut range = self.config.max_block_range.max(1);
        let mut start = from;

        while start <= to {
            let end = to.min(start.saturating_add(range - 1));

            let filter = Filter::new()
                .address(self.config.contract)
                .from_block(start)
                .to_block(end);

            match timeout(RPC_TIMEOUT, provider.get_logs(&filter))
                .await
                .context("eth_getLogs timed out")?
            {
                Ok(chunk) => {
                    logs.extend(chunk);
                    start = end + 1;
                }
                // most likely the range is more than this node is willing to
                // serve, try again with a smaller one
                Err(error) if is_rpc_error(&error) && end > start => {
                    range = (end - start).div_ceil(2);

                    warn!("eth_getLogs for {start}..={end} failed ({error}), retrying with {range} blocks at a time");
                }
                Err(error) => return Err(error).context("eth_getLogs failed"),
            }
        }

        Ok(logs)
    }

    /// Applies every log up to the current head that has not been applied yet.
    async fn sync(&mut self) -> anyhow::Result<()> {
        let provider = self.state.service.rpc.get_instance();

        let head = timeout(RPC_TIMEOUT, provider.get_block_number())
            .await
            .context("eth_blockNumber timed out")?
            .context("eth_blockNumber failed")?
            .as_u64();

        let cursor = if let Some(cursor) = self.cursor {
            cursor
        } else {
            let persisted = self
                .state
                .service
                .cache
                .get(CURSOR_KEY)
                .await
                .ok()
                .and_then(|cursor| cursor.parse().ok());

            let cursor = start_block(persisted, head, self.config.cache_ttl);

            // Nothing read before the fence is cached. A fence ahead of the
            // chain can only come from another network sharing this Redis.
            let fence = self.state.service.cache.get(FENCE_KEY).await.ok();

            if let Some(fence) = fence.and_then(|fence| fence.parse::<u64>().ok()) {
                if fence > head.saturating_add(self.config.reorg_depth) {
                    warn!(
                        fence,
                        head,
                        "The cache holds state of a chain that is ahead of this one, nothing will be cached until it is flushed"
                    );
                }
            }

            info!(
                head,
                persisted, "Catching up on NameNFT logs since block {cursor}"
            );

            cursor
        };

        // re-read the blocks that may still be reorganized
        let from = (cursor + 1).saturating_sub(self.config.reorg_depth);

        // this node is further behind than that, leave it to the next round
        if head < from {
            return Ok(());
        }

        let logs = self.fetch_logs(&provider, from, head).await?;

        let reconciled = reconcile(&self.seen, &logs, from, head);

        self.apply(&reconciled.batch).await?;

        for (block, event) in &reconciled.fresh {
            self.record(event, *block, false);
        }

        // logs beyond this node's head came from the subscription, keep them
        self.seen.retain(|_, (block, _)| *block > head);
        self.seen.extend(reconciled.current);

        let cursor = cursor.max(head);

        self.cursor = Some(cursor);
        self.state
            .metrics
            .watcher_block
            .set(i64::try_from(cursor).unwrap_or(i64::MAX));

        debug!(from, head, logs = logs.len(), "Synced NameNFT logs");

        self.state
            .service
            .cache
            .set(
                CURSOR_KEY,
                &cursor.to_string(),
                u32::try_from(CURSOR_TTL.max(self.config.cache_ttl)).unwrap_or(u32::MAX),
            )
            .await
            .map_err(|error| anyhow::anyhow!("failed to persist cursor: {error:?}"))
    }

    /// Applies a log pushed by the subscription.
    async fn push(&mut self, log: &Log) -> anyhow::Result<()> {
        let Some((id, block, event)) = identify(log) else {
            return Ok(());
        };

        let invalidation = event.invalidation();
        let removed = log.removed.unwrap_or(false);
        let mut batch = Batch::default();

        if removed {
            batch.reorged(&invalidation, block);
        } else if self.seen.contains_key(&id) {
            return Ok(());
        } else {
            batch.mined(&invalidation, block);
        }

        self.apply(&batch).await?;
        self.record(&event, block, removed);

        if removed {
            self.seen.remove(&id);
        } else {
            self.seen.insert(id, (block, invalidation));
        }

        Ok(())
    }
}

/// Forwards the logs of `contract` from a WebSocket subscription into `logs`,
/// reconnecting whenever the subscription breaks.
async fn subscribe(
    url: String,
    contract: Address,
    logs: mpsc::Sender<Log>,
    shutdown: CancellationToken,
) {
    loop {
        match Provider::<Ws>::connect(&url).await {
            Ok(provider) => match provider
                .subscribe_logs(&Filter::new().address(contract))
                .await
            {
                Ok(mut stream) => {
                    info!("Subscribed to NameNFT logs over WebSocket");

                    loop {
                        tokio::select! {
                            log = stream.next() => {
                                let Some(log) = log else { break };

                                if logs.send(log).await.is_err() {
                                    return;
                                }
                            }
                            () = shutdown.cancelled() => return,
                        }
                    }

                    warn!("WebSocket log subscription ended, polling until it is back");
                }
                Err(error) => {
                    warn!("WebSocket log subscription failed ({error}), polling instead");
                }
            },
            Err(error) => {
                warn!("WebSocket connection failed ({error}), polling instead");
            }
        }

        tokio::select! {
            () = tokio::time::sleep(WS_RETRY) => {}
            () = shutdown.cancelled() => return,
        }
    }
}

/// Runs the watcher until `shutdown` is cancelled.
pub async fn run(state: Arc<AppState>, config: Config, shutdown: CancellationToken) {
    info!(
        contract = ?config.contract,
        poll_interval = ?config.poll_interval,
        reorg_depth = config.reorg_depth,
        websocket = config.ws_url.is_some(),
        "Watching NameNFT logs"
    );

    let (log_tx, mut log_rx) = mpsc::channel(256);

    if let Some(url) = config.ws_url.clone() {
        tokio::spawn(subscribe(url, config.contract, log_tx, shutdown.clone()));
    } else {
        drop(log_tx);
    }

    let mut interval = tokio::time::interval(config.poll_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut subscribed = true;

    let mut watcher = Watcher {
        state,
        config,
        seen: HashMap::new(),
        cursor: None,
    };

    loop {
        tokio::select! {
            () = shutdown.cancelled() => break,
            _ = interval.tick() => {
                // nothing is remembered before it has been applied, so a
                // round can be abandoned at any point
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    result = watcher.sync() => {
                        if let Err(error) = result {
                            warn!("Failed to sync NameNFT logs: {error:#}");
                        }
                    }
                }
            }
            log = log_rx.recv(), if subscribed => {
                let Some(log) = log else {
                    subscribed = false;
                    continue;
                };

                // not applied means not remembered: the next poll retries
                if let Err(error) = watcher.push(&log).await {
                    warn!("Failed to apply NameNFT log: {error:#}");
                }
            }
        }
    }

    info!("Watcher stopped");
}

#[cfg(test)]
mod tests {
    use ethers::types::{Bytes, U64};

    use super::*;

    const NODE_A: &str = "0x41593da8bfd4bccd6b82a0f4edd9adacf5a92212c6238c69b4704fd763bcc162";
    const NODE_B: &str = "0xe8ecd4ddb31a1590381a227f52e90899015af42fb8a1cdbc140c0463d603334d";

    // AddrChanged(node, address)
    fn addr_changed(node: &str, block: u64, fork: u8, index: u64) -> Log {
        Log {
            topics: vec![
                "0x52d7d861f09ab3d26239d492e8968629f95e9e318cf0b73bfddc441522a15fd2"
                    .parse()
                    .unwrap(),
                node.parse().unwrap(),
            ],
            data: Bytes::from(vec![0u8; 32]),
            block_number: Some(U64::from(block)),
            block_hash: Some(H256::repeat_byte(fork)),
            log_index: Some(U256::from(index)),
            ..Log::default()
        }
    }

    fn tag(node: &str) -> String {
        enstate_shared::core::gns::token_tag(&node.parse().unwrap())
    }

    #[test]
    fn test_start_block() {
        // first start: go back as far as a cache entry can have lived
        assert_eq!(start_block(None, 10_000, 600), 10_000 - 51);
        assert_eq!(start_block(None, 10_000, 86_400), 10_000 - 7_201);
        // restart: continue from the cursor ...
        assert_eq!(start_block(Some(9_990), 10_000, 600), 9_990);
        // ... unless everything cached back then has expired anyway
        assert_eq!(start_block(Some(5_000), 10_000, 600), 10_000 - 51);
        // a cursor ahead of the chain (other network, node behind) is not trusted
        assert_eq!(start_block(Some(20_000), 10_000, 600), 10_000);
        // young chain
        assert_eq!(start_block(None, 10, 600), 0);
    }

    #[test]
    fn test_reconcile_new_logs_once() {
        let logs = vec![
            addr_changed(NODE_A, 100, 1, 0),
            addr_changed(NODE_B, 101, 2, 3),
        ];

        let Reconciled {
            batch,
            current,
            fresh,
        } = reconcile(&Seen::new(), &logs, 90, 101);

        assert_eq!(fresh.len(), 2);
        assert_eq!(fresh[1].0, 101);
        assert_eq!(current.len(), 2);
        assert_eq!(
            batch.invalidation.tags.iter().cloned().collect::<Vec<_>>(),
            vec![tag(NODE_A), tag(NODE_B)]
        );
        // reads from block 101 onwards have seen both events
        assert_eq!(batch.block, 101);

        // the same logs again (overlapping window): nothing to do
        let Reconciled {
            batch,
            current,
            fresh,
        } = reconcile(&current, &logs, 92, 103);

        assert!(fresh.is_empty());
        assert!(batch.invalidation.is_empty());
        assert_eq!(current.len(), 2);
    }

    #[test]
    fn test_reconcile_reorg() {
        let seen = reconcile(
            &Seen::new(),
            &[
                addr_changed(NODE_A, 100, 1, 0),
                addr_changed(NODE_B, 80, 9, 0),
            ],
            80,
            100,
        )
        .current;

        // block 100 was replaced, and the transaction did not make it into the
        // new block: whatever was cached from the orphaned state must go
        let Reconciled {
            batch,
            current,
            fresh,
        } = reconcile(&seen, &[], 95, 102);

        assert!(fresh.is_empty());
        assert!(current.is_empty());
        assert_eq!(
            batch.invalidation.tags.iter().cloned().collect::<Vec<_>>(),
            vec![tag(NODE_A)]
        );
        // nothing read up to the current head can be trusted
        assert_eq!(batch.block, 103);

        // the transaction was included again in the replacement block
        let Reconciled {
            batch,
            current,
            fresh,
        } = reconcile(&seen, &[addr_changed(NODE_A, 100, 2, 5)], 95, 102);

        assert_eq!(fresh.len(), 1);
        assert_eq!(current.len(), 1);
        assert_eq!(
            batch.invalidation.tags.iter().cloned().collect::<Vec<_>>(),
            vec![tag(NODE_A)]
        );
        assert_eq!(batch.block, 103);

        // logs outside the window that was read say nothing: a node that is
        // behind does not make the logs of newer blocks disappear
        let batch = reconcile(&seen, &[], 85, 99).batch;
        assert!(batch.invalidation.is_empty());
    }

    #[test]
    fn test_reconcile_skips_unusable_logs() {
        let mut pending = addr_changed(NODE_A, 100, 1, 0);
        pending.block_hash = None;

        // Committed(bytes32,address) does not affect resolution
        let mut committed = addr_changed(NODE_A, 100, 1, 1);
        committed.topics[0] = H256(ethers_core::utils::keccak256("Committed(bytes32,address)"));

        let Reconciled {
            batch,
            current,
            fresh,
        } = reconcile(&Seen::new(), &[pending, committed], 90, 100);

        assert!(batch.invalidation.is_empty());
        assert!(current.is_empty());
        assert!(fresh.is_empty());
    }
}
