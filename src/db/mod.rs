use std::collections::HashMap;

use db_key::Key;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(feature = "node")]
pub use disk::*;
pub use ram::*;
use Change::{Remove, Set};

use crate::core::{Account, Address, Block, Hasher, Header, Money};
use crate::crypto::merkle::MerkleTree;
use crate::db::key_space::KeySpace;

pub mod key_space;

pub type Result<T> = core::result::Result<T, KvStoreError>;

#[derive(Clone)]
pub enum Change {
    Set(KeySpace, Vec<u8>, Blob),
    Remove(KeySpace, Vec<u8>),
}

#[derive(Clone, Default)]
pub struct Batch(pub Vec<Change>);

impl Batch {
    pub fn new() -> Self {
        Batch(Vec::new())
    }

    pub fn set(&mut self, space: KeySpace, key: &[u8], value: Blob) {
        self.0.push(Set(space, key.to_vec(), value))
    }

    pub fn remove(&mut self, space: KeySpace, key: &[u8]) {
        self.0.push(Remove(space, key.to_vec()))
    }
}

pub trait Database: Sync + Send {
    fn backend() -> &'static str;
    fn get(&self, space: KeySpace, key: &[u8]) -> Result<Option<Blob>>;
    fn batch(&self, batch: &Batch) -> Result<()>;
}

#[derive(Error, Debug)]
pub enum KvStoreError {
    #[error("kvstore failure")]
    Failure,
    #[error("kvstore data corrupted")]
    Corrupted(#[from] bincode::Error),
    #[error("{0}")]
    Custom(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StringKey(String);

impl StringKey {
    pub fn new(s: &str) -> StringKey {
        StringKey(s.to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blob(pub Vec<u8>);

impl From<Vec<u8>> for Blob {
    fn from(v: Vec<u8>) -> Self {
        Blob(v)
    }
}

macro_rules! gen_try_into {
    ( $( $x:ty ),* ) => {
        $(
            impl TryInto<$x> for Blob {
                type Error = KvStoreError;
                fn try_into(self) -> core::result::Result<$x, Self::Error> {
                    Ok(bincode::deserialize(&self.0)?)
                }
            }
        )*
    };
}

gen_try_into!(
    u32,
    u64,
    usize,
    Account,
    Block,
    Vec<WriteOp>,
    MerkleTree<Hasher>
);

gen_from!(
    u32,
    u64,
    usize,
    Account,
    &Block,
    Vec<WriteOp>,
    MerkleTree<Hasher>
);

impl Key for StringKey {
    fn from_u8(key: &[u8]) -> StringKey {
        StringKey(std::str::from_utf8(key).unwrap().to_string())
    }

    fn as_slice<T, F: Fn(&[u8]) -> T>(&self, f: F) -> T {
        f(&self.0.as_bytes())
    }
}

impl From<String> for StringKey {
    fn from(s: String) -> Self {
        Self::new(&s)
    }
}

impl From<&str> for StringKey {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WriteOp {
    Remove(StringKey),
    Put(StringKey, Blob),
}

pub trait KvStore {
    fn get(&self, k: StringKey) -> core::result::Result<Option<Blob>, KvStoreError>;
    fn update(&mut self, ops: &Vec<WriteOp>) -> core::result::Result<(), KvStoreError>;
    fn rollback_of(&self, ops: &Vec<WriteOp>) -> core::result::Result<Vec<WriteOp>, KvStoreError> {
        let mut rollback = Vec::new();
        for op in ops.iter() {
            let key = match op {
                WriteOp::Put(k, _) => k,
                WriteOp::Remove(k) => k,
            }
            .clone();
            rollback.push(match self.get(key.clone())? {
                Some(b) => WriteOp::Put(key, b.clone()),
                None => WriteOp::Remove(key),
            })
        }
        Ok(rollback)
    }
}

pub struct LruCacheKvStore<K: KvStore> {
    store: K,
    cache: LruCache<String, Option<Blob>>,
}

impl<K: KvStore> LruCacheKvStore<K> {
    pub fn new(store: K, cap: usize) -> Self {
        Self {
            store,
            cache: LruCache::new(cap),
        }
    }
}

impl<K: KvStore> KvStore for LruCacheKvStore<K> {
    fn get(&self, k: StringKey) -> core::result::Result<Option<Blob>, KvStoreError> {
        unsafe {
            let mutable = &mut *(self as *const Self as *mut Self);
            if let Some(v) = mutable.cache.get(&k.0) {
                Ok(v.clone())
            } else {
                let res = mutable.store.get(k.clone())?;
                mutable.cache.put(k.0.clone(), res.clone());
                Ok(res)
            }
        }
    }
    fn update(&mut self, ops: &Vec<WriteOp>) -> core::result::Result<(), KvStoreError> {
        for op in ops.into_iter() {
            match op {
                WriteOp::Remove(k) => self.cache.pop(&k.0),
                WriteOp::Put(k, _) => self.cache.pop(&k.0),
            };
        }
        self.store.update(ops)
    }
}

pub struct RamMirrorKvStore<'a, K: KvStore> {
    store: &'a K,
    overwrite: HashMap<String, Option<Blob>>,
}

impl<'a, K: KvStore> RamMirrorKvStore<'a, K> {
    pub fn new(store: &'a K) -> Self {
        Self {
            store,
            overwrite: HashMap::new(),
        }
    }
    pub fn to_ops(self) -> Vec<WriteOp> {
        self.overwrite
            .into_iter()
            .map(|(k, v)| match v {
                Some(b) => WriteOp::Put(k.into(), b),
                None => WriteOp::Remove(k.into()),
            })
            .collect()
    }
}

impl<'a, K: KvStore> KvStore for RamMirrorKvStore<'a, K> {
    fn get(&self, k: StringKey) -> core::result::Result<Option<Blob>, KvStoreError> {
        if self.overwrite.contains_key(&k.0) {
            Ok(self.overwrite.get(&k.0).cloned().unwrap())
        } else {
            self.store.get(k)
        }
    }
    fn update(&mut self, ops: &Vec<WriteOp>) -> core::result::Result<(), KvStoreError> {
        for op in ops.into_iter() {
            match op {
                WriteOp::Remove(k) => self.overwrite.insert(k.0.clone(), None),
                WriteOp::Put(k, v) => self.overwrite.insert(k.0.clone(), Some(v.clone())),
            };
        }
        Ok(())
    }
}

#[cfg(feature = "node")]
mod disk;
mod ram;

#[inline]
pub fn number_key<N: TryInto<u64>>(n: N) -> Result<[u8; 8]>
where
    <N as TryInto<u64>>::Error: std::fmt::Display,
{
    let n = n
        .try_into()
        .map_err(|e| KvStoreError::Custom(format!("{}", e)))?;
    Ok([
        (n >> 56) as u8,
        ((n >> 48) & 0xff) as u8,
        ((n >> 40) & 0xff) as u8,
        ((n >> 32) & 0xff) as u8,
        ((n >> 24) & 0xff) as u8,
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
    ])
}
