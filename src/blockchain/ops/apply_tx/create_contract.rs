use super::*;

pub fn create_contract<K: KvStore>(
    chain: &mut KvStoreChain<K>,
    contract_id: ContractId,
    contract: &zk::ZkContract,
    state: &Option<zk::ZkDataPairs>,
) -> Result<(), BlockchainError> {
    if !contract.state_model.is_valid::<CoreZkHasher>() {
        return Err(BlockchainError::InvalidStateModel);
    }
    chain.database.update(&[WriteOp::Put(
        keys::contract(&contract_id),
        contract.clone().into(),
    )])?;
    chain.database.update(&[WriteOp::Put(
        keys::contract_account(&contract_id),
        ContractAccount {
            compressed_state: contract.initial_state,
            height: 1,
        }
        .into(),
    )])?;
    zk::KvStoreStateManager::<CoreZkHasher>::update_contract(
        &mut chain.database,
        contract_id,
        &state
            .clone()
            .ok_or(BlockchainError::StateNotGiven)?
            .as_delta(),
        1,
    )?;
    if zk::KvStoreStateManager::<CoreZkHasher>::root(&mut chain.database, contract_id)?
        != contract.initial_state
    {
        return Err(BlockchainError::InvalidState);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{RamKvStore, WriteOp};

    #[test]
    fn test_create_contract() {
        let chain = KvStoreChain::new(
            RamKvStore::new(),
            crate::config::blockchain::get_test_blockchain_config(),
        )
        .unwrap();
        let contract_id: ContractId =
            "0001020304050607080900010203040506070809000102030405060708090001"
                .parse()
                .unwrap();
        let state_model = zk::ZkStateModel::Struct {
            field_types: vec![zk::ZkStateModel::Scalar, zk::ZkStateModel::Scalar],
        };
        let initial_state =
            zk::ZkCompressedState::empty::<crate::core::ZkHasher>(state_model.clone());
        let contract = zk::ZkContract {
            state_model,
            initial_state: initial_state.clone(),
            deposit_functions: vec![],
            withdraw_functions: vec![],
            functions: vec![],
        };
        let (ops, _) = chain
            .isolated(|chain| {
                Ok(create_contract(
                    chain,
                    contract_id,
                    &contract,
                    &Some(Default::default()),
                )?)
            })
            .unwrap();

        let expected_ops = vec![
            WriteOp::Put(
                "CAC-0001020304050607080900010203040506070809000102030405060708090001".into(),
                ContractAccount {
                    height: 1,
                    compressed_state: initial_state.clone(),
                }
                .into(),
            ),
            WriteOp::Put(
                "CON-0001020304050607080900010203040506070809000102030405060708090001".into(),
                contract.into(),
            ),
            WriteOp::Put(
                "S-0001020304050607080900010203040506070809000102030405060708090001-HGT".into(),
                1u64.into(),
            ),
            WriteOp::Put(
                "S-0001020304050607080900010203040506070809000102030405060708090001-RT".into(),
                initial_state.into(),
            ),
        ];

        assert_eq!(ops, expected_ops);
    }
}
