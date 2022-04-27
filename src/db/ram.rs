use std::collections::{BTreeMap, HashMap};

use parking_lot::RwLock;

use super::*;

pub struct RamKvStore(HashMap<String, Blob>);
impl RamKvStore {
    pub fn new() -> RamKvStore {
        RamKvStore(HashMap::new())
    }
}

impl KvStore for RamKvStore {
    fn get(&self, k: StringKey) -> core::result::Result<Option<Blob>, KvStoreError> {
        Ok(self.0.get(&k.0).cloned())
    }
    fn update(&mut self, ops: &Vec<WriteOp>) -> core::result::Result<(), KvStoreError> {
        for op in ops.iter() {
            match op {
                WriteOp::Remove(k) => self.0.remove(&k.0),
                WriteOp::Put(k, v) => self.0.insert(k.0.clone(), v.clone()),
            };
        }
        Ok(())
    }
}

pub struct Memory(RwLock<HashMap<KeySpace, BTreeMap<Vec<u8>, Blob>>>);

impl Database for Memory {
    fn backend() -> &'static str {
        "memory"
    }

    fn get(&self, space: KeySpace, key: &[u8]) -> Result<Option<Blob>> {
        let db = self.0.read();
        if let Some(column) = db.get(&space) {
            Ok(column.get(key).map(|b| b.clone()))
        } else {
            Ok(None)
        }
    }

    fn batch(&self, batch: &Batch) -> Result<()> {
        let mut db = self.0.write();
        for change in batch.0.iter() {
            match change {
                Set(key_space, key, value) => {
                    db.entry(key_space.clone())
                        .or_default()
                        .insert(key.clone(), value.clone());
                }
                Remove(key_space, key) => {
                    db.entry(key_space.clone()).and_modify(|column| {
                        column.remove(key);
                    });
                }
            }
        }
        Ok(())
    }
}
