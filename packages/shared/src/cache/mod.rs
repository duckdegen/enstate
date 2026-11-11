use std::fmt::Debug;

use async_trait::async_trait;

#[derive(Debug)]
pub enum CacheError {
    Other(String),
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait CacheLayer: Send + Sync {
    async fn get(&self, key: &str) -> Result<String, CacheError>;
    async fn set(&self, key: &str, value: &str, expires: u32) -> Result<(), CacheError>;
    async fn cache_hit(&self, key: &str) -> Result<(), CacheError>;

    /// Like `set`, but registers `key` under every tag in `tags` so that
    /// `invalidate` can drop it later. `block` is the block the value was read
    /// at: the write is refused (`Ok(false)`) when an invalidation for a later
    /// block has already been applied, as the value may predate the change
    /// that invalidation was about.
    async fn set_tagged(
        &self,
        key: &str,
        value: &str,
        expires: u32,
        tags: &[String],
        block: u64,
    ) -> Result<bool, CacheError>;

    /// Atomically deletes `keys` and every entry registered under one of
    /// `tags`, and remembers `block` so that values read before it are no
    /// longer accepted by `set_tagged`. Returns the number of deleted entries.
    async fn invalidate(
        &self,
        tags: &[String],
        keys: &[String],
        block: u64,
    ) -> Result<u64, CacheError>;
}

pub struct PassthroughCacheLayer {}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl CacheLayer for PassthroughCacheLayer {
    async fn get(&self, _key: &str) -> Result<String, CacheError> {
        Err(CacheError::Other("".to_string()))
    }

    async fn set(&self, _key: &str, _value: &str, _expires: u32) -> Result<(), CacheError> {
        Ok(())
    }

    async fn cache_hit(&self, _key: &str) -> Result<(), CacheError> {
        Ok(())
    }

    async fn set_tagged(
        &self,
        _key: &str,
        _value: &str,
        _expires: u32,
        _tags: &[String],
        _block: u64,
    ) -> Result<bool, CacheError> {
        Ok(false)
    }

    async fn invalidate(
        &self,
        _tags: &[String],
        _keys: &[String],
        _block: u64,
    ) -> Result<u64, CacheError> {
        Ok(0)
    }
}
