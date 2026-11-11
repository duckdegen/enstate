use std::num::TryFromIntError;

use axum::async_trait;
use enstate_shared::cache::{CacheError, CacheLayer};
use lazy_static::lazy_static;
use redis::{aio::ConnectionManager, AsyncCommands, Script};

/// Highest block an invalidation has been applied for. Values read at an
/// earlier block are refused by `set_tagged`: they may predate the change.
pub const FENCE_KEY: &str = "gwei:fence";

lazy_static! {
    /// KEYS: fence, entry, tags...  ARGV: block, value, ttl
    ///
    /// A tag is a set of the entry keys registered under it. It is kept alive
    /// at least as long as its longest-lived member.
    static ref SET_TAGGED: Script = Script::new(
        r"
        if tonumber(redis.call('GET', KEYS[1]) or '0') > tonumber(ARGV[1]) then
            return 0
        end

        local ttl = tonumber(ARGV[3])

        redis.call('SET', KEYS[2], ARGV[2], 'EX', ttl)

        for i = 3, #KEYS do
            redis.call('SADD', KEYS[i], KEYS[2])

            if redis.call('TTL', KEYS[i]) < ttl then
                redis.call('EXPIRE', KEYS[i], ttl)
            end
        end

        return 1
        ",
    );

    /// KEYS: fence, tags..., entries...  ARGV: block, number of tags
    static ref INVALIDATE: Script = Script::new(
        r"
        if tonumber(ARGV[1]) > tonumber(redis.call('GET', KEYS[1]) or '0') then
            redis.call('SET', KEYS[1], ARGV[1])
        end

        local tags = tonumber(ARGV[2])
        local removed = 0

        for i = 2, #KEYS do
            if i <= tags + 1 then
                for _, entry in ipairs(redis.call('SMEMBERS', KEYS[i])) do
                    removed = removed + redis.call('DEL', entry)
                end

                redis.call('DEL', KEYS[i])
            else
                removed = removed + redis.call('DEL', KEYS[i])
            end
        end

        return removed
        ",
    );
}

#[derive(Clone)]
pub struct Redis {
    redis: ConnectionManager,
}

impl Redis {
    pub const fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }
}

#[async_trait]
impl CacheLayer for Redis {
    async fn get(&self, key: &str) -> Result<String, CacheError> {
        let mut redis = self.redis.clone();

        let x: Result<String, _> = redis.get(key).await;

        match x {
            Ok(x) => Ok(x),
            Err(error) => Err(CacheError::Other(error.to_string())),
        }
    }

    async fn set(&self, key: &str, value: &str, expires: u32) -> Result<(), CacheError> {
        let mut redis = self.redis.clone();

        let x: Result<(), _> = redis
            .set_ex(
                key,
                value,
                expires.into(),
            )
            .await;

        match x {
            Ok(()) => Ok(()),
            Err(error) => Err(CacheError::Other(error.to_string())),
        }
    }

    async fn cache_hit(&self, key: &str) -> Result<(), CacheError> {
        let mut redis = self.redis.clone();

        let x: Result<(), _> = redis.zadd("cache_hits", key, 1).await;

        match x {
            Ok(x) => Ok(x),
            Err(error) => Err(CacheError::Other(error.to_string())),
        }
    }

    async fn set_tagged(
        &self,
        key: &str,
        value: &str,
        expires: u32,
        tags: &[String],
        block: u64,
    ) -> Result<bool, CacheError> {
        let mut redis = self.redis.clone();

        SET_TAGGED
            .key(FENCE_KEY)
            .key(key)
            .key(tags)
            .arg(block)
            .arg(value)
            .arg(expires)
            .invoke_async(&mut redis)
            .await
            .map_err(|error| CacheError::Other(error.to_string()))
    }

    async fn invalidate(
        &self,
        tags: &[String],
        keys: &[String],
        block: u64,
    ) -> Result<u64, CacheError> {
        let mut redis = self.redis.clone();

        INVALIDATE
            .key(FENCE_KEY)
            .key(tags)
            .key(keys)
            .arg(block)
            .arg(tags.len())
            .invoke_async(&mut redis)
            .await
            .map_err(|error| CacheError::Other(error.to_string()))
    }
}

/// These run against a real Redis and are skipped unless `TEST_REDIS_URL` is
/// set, e.g. `TEST_REDIS_URL=redis://127.0.0.1:6390 cargo test -p enstate`.
#[cfg(test)]
mod tests {
    use enstate_shared::cache::CacheLayer;
    use redis::AsyncCommands;

    use super::{Redis, FENCE_KEY};

    async fn setup() -> Option<(Redis, redis::aio::ConnectionManager)> {
        let url = std::env::var("TEST_REDIS_URL").ok()?;
        let client = redis::Client::open(url).unwrap();
        let mut connection = redis::aio::ConnectionManager::new(client).await.unwrap();

        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut connection)
            .await
            .unwrap();

        Some((Redis::new(connection.clone()), connection))
    }

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(ToString::to_string).collect()
    }

    // one test: they share a database
    #[tokio::test]
    async fn test_tagged_cache() {
        let Some((cache, mut redis)) = setup().await else {
            eprintln!("TEST_REDIS_URL not set, skipping");
            return;
        };

        // entries are stored with their TTL and registered under their tags
        assert!(cache
            .set_tagged("k1", "v1", 100, &strings(&["t:a", "s:b"]), 10)
            .await
            .unwrap());
        assert!(cache
            .set_tagged("k2", "v2", 50, &strings(&["t:a"]), 10)
            .await
            .unwrap());
        assert!(cache.set_tagged("k3", "v3", 100, &[], 10).await.unwrap());

        assert_eq!(cache.get("k1").await.unwrap(), "v1");
        let ttl: i64 = redis.ttl("k2").await.unwrap();
        assert!((1..=50).contains(&ttl));

        // a tag outlives its longest-lived member
        let ttl: i64 = redis.ttl("t:a").await.unwrap();
        assert!((51..=100).contains(&ttl));

        // invalidating a tag drops exactly the entries registered under it
        assert_eq!(
            cache.invalidate(&strings(&["s:b"]), &[], 11).await.unwrap(),
            1
        );
        assert!(cache.get("k1").await.is_err());
        assert_eq!(cache.get("k2").await.unwrap(), "v2");
        assert_eq!(cache.get("k3").await.unwrap(), "v3");
        let exists: bool = redis.exists("s:b").await.unwrap();
        assert!(!exists);

        // ... plain keys can be dropped alongside, unknown tags are fine
        assert_eq!(
            cache
                .invalidate(&strings(&["t:a", "t:unknown"]), &strings(&["k3", "k4"]), 12)
                .await
                .unwrap(),
            2
        );
        assert!(cache.get("k2").await.is_err());
        assert!(cache.get("k3").await.is_err());

        // values read before the last invalidation are refused ...
        let fence: u64 = redis.get(FENCE_KEY).await.unwrap();
        assert_eq!(fence, 12);
        assert!(!cache
            .set_tagged("k1", "stale", 100, &strings(&["t:a"]), 11)
            .await
            .unwrap());
        assert!(cache.get("k1").await.is_err());
        let exists: bool = redis.exists("t:a").await.unwrap();
        assert!(!exists);

        // ... values read at or after it are accepted
        assert!(cache
            .set_tagged("k1", "fresh", 100, &strings(&["t:a"]), 12)
            .await
            .unwrap());
        assert_eq!(cache.get("k1").await.unwrap(), "fresh");

        // the fence never moves backwards
        assert_eq!(cache.invalidate(&[], &[], 5).await.unwrap(), 0);
        let fence: u64 = redis.get(FENCE_KEY).await.unwrap();
        assert_eq!(fence, 12);
    }
}
