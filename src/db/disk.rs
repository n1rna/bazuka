use std::fmt::format;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use leveldb::batch::Batch;
use leveldb::database::batch::Writebatch;
use leveldb::database::Database;
use leveldb::kv::KV;
use leveldb::options::{Options, ReadOptions, WriteOptions};

use crate::blockchain::BlockchainError::KvStoreError;

use super::*;

pub struct LevelDbKvStore(Database<StringKey>);
impl LevelDbKvStore {
    pub fn new(path: &Path) -> LevelDbKvStore {
        fs::create_dir_all(&path).unwrap();
        let mut options = Options::new();
        options.create_if_missing = true;
        LevelDbKvStore(Database::open(&path, options).unwrap())
    }
}

impl KvStore for LevelDbKvStore {
    fn get(&self, k: StringKey) -> Result<Option<Blob>> {
        let read_opts = ReadOptions::new();
        match self.0.get(read_opts, k) {
            Ok(v) => Ok(v.map(|v| Blob(v))),
            Err(_) => Err(KvStoreError::Failure),
        }
    }
    fn update(&mut self, ops: &Vec<WriteOp>) -> Result<()> {
        let write_opts = WriteOptions::new();
        let mut batch = Writebatch::new();
        for op in ops.iter() {
            match op {
                WriteOp::Remove(k) => batch.delete(k.clone()),
                WriteOp::Put(k, v) => batch.put(k.clone(), &v.0),
            }
        }
        match self.0.write(write_opts, &batch) {
            Ok(_) => Ok(()),
            Err(_) => Err(KvStoreError::Failure),
        }
    }
}

pub struct LevelDB(Arc<Database<Vec<u8>>>);
impl LevelDB {
    pub fn new(path: &Path) -> LevelDB {
        fs::create_dir_all(&path).unwrap();
        let mut options = Options::new();
        options.create_if_missing = true;
        LevelDB(Arc::new(Database::open(&path, options).unwrap()))
    }
}

impl super::Database for LevelDB {
    fn backend() -> &'static str {
        "levelDB"
    }

    fn get(&self, space: KeySpace, key: &[u8]) -> Result<Option<Blob>> {
        let key = key_with_prefix(key, space);
        Ok(self
            .0
            .get(ReadOptions::new(), &key)
            .map_err(|e| KvStoreError::Custom(format!("{}", e)))?
            .into())
    }

    fn batch(&self, batch: &super::Batch) -> Result<()> {
        let write_opts = WriteOptions::new();
        let mut write_batch = Writebatch::new();
        for change in batch.0.iter() {
            match change {
                Set(key_space, key, value) => {
                    let key = key_with_prefix(key, key_space.clone());
                    write_batch.put(key, &value.0)
                }
                Remove(key_space, key) => {
                    let key = key_with_prefix(key, key_space.clone());
                    write_batch.delete(key)
                }
            }
        }
        Ok(self
            .0
            .write(write_opts, &write_batch)
            .map_err(|e| KvStoreError::Custom(format!("{}", e)))?)
    }
}

#[inline]
fn key_with_prefix(key: &[u8], space: KeySpace) -> Vec<u8> {
    let mut ret = Vec::with_capacity(key.len() + 1);
    ret.push(space.into());
    ret.copy_from_slice(key);
    ret
}
