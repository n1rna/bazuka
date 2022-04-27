use lru::LruCache;
use parking_lot::RwLock;

use crate::core::{Hash, Header};

#[derive(Clone)]
pub struct HeaderMeta {
    pub hash: Hash,
    pub number: u64,
    pub parent: Hash,
    pub state_root: Hash,
}

impl From<&Header> for HeaderMeta {
    fn from(header: &Header) -> Self {
        HeaderMeta {
            hash: header.hash().clone(),
            number: header.number.clone(),
            parent: header.parent_hash.clone(),
            state_root: header.state_root.clone(),
        }
    }
}

pub struct HeaderMetaCache(RwLock<LruCache<Hash, HeaderMeta>>);

impl Default for HeaderMetaCache {
    fn default() -> Self {
        Self(RwLock::new(LruCache::new(2048)))
    }
}

impl HeaderMetaCache {
    /// Creates a new LRU header metadata cache with `capacity`.
    pub fn new(capacity: usize) -> Self {
        HeaderMetaCache(RwLock::new(LruCache::new(capacity)))
    }

    pub fn header_metadata(&self, hash: Hash) -> Option<HeaderMeta> {
        self.0.write().get(&hash).cloned()
    }

    pub fn insert_header_metadata(&self, hash: Hash, h: HeaderMeta) {
        self.0.write().put(hash, h);
    }

    pub fn remove_header_metadata(&self, hash: Hash) {
        self.0.write().pop(&hash);
    }
}
