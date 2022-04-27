use core::fmt;
use std::fmt::write;

use serde::{Deserialize, Serialize};

use crate::core::hash::Hashing;
use crate::core::{AutoDeserialize, AutoHash, AutoSerialize, Hash, MemberBound};
use crate::crypto::merkle::MerkleTree;
use crate::crypto::SignatureScheme;

use super::header::Header;
use super::transaction::Transaction;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<H: Hashing, S: SignatureScheme> {
    pub header: Header<H>,
    pub body: Vec<Transaction<S>>,
}

impl<H: Hashing, S: SignatureScheme> Block<H, S> {
    pub fn merkle_tree(&self) -> MerkleTree<H> {
        MerkleTree::<H>::new(self.body.iter().map(|tx| tx.hash::<H>()).collect())
    }
}

#[derive(PartialEq, Eq, Clone)]
pub enum BlockId {
    Hash(Hash),
    Number(u64),
}

impl BlockId {
    pub fn hash(hash: Hash) -> Self {
        BlockId::Hash(hash)
    }

    pub fn number(number: u64) -> Self {
        BlockId::Number(number)
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BlockId::Hash(h) => {
                write!(f, "{:?}", h)
            }
            BlockId::Number(n) => {
                write!(f, "{:?}", n)
            }
        }
    }
}
