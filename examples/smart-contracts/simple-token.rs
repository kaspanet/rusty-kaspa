use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput, TransactionOutpoint};
use kaspa_hashes::Hash;
use kaspa_txscript::script_builder::ScriptBuilder;

pub mod simple_token {
    use super::*;

pub fn create_contract_deployment_transaction(
    contract_code: &[u8],
    deployer_utxo: TransactionOutpoint,
    deployer_amount: u64,
    fee: u64,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let mut script_builder = ScriptBuilder::new();
    
    script_builder.add_data(contract_code)?;
    script_builder.add_op(0xc0)?;
    
    let script_public_key = script_builder.drain();
    
    let input = TransactionInput {
        previous_outpoint: deployer_utxo,
        signature_script: vec![],
        sequence: 0,
        sig_op_count: 0,
    };
    
    let output = TransactionOutput {
        value: deployer_amount - fee,
        script_public_key,
    };
    
    Ok(Transaction::new(
        0,
        vec![input],
        vec![output],
        0,
        Hash::from_u64_word(0),
        0,
        vec![],
    ))
}

pub fn create_contract_call_transaction(
    contract_address: Hash,
    function_data: &[u8],
    caller_utxo: TransactionOutpoint,
    caller_amount: u64,
    fee: u64,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let mut script_builder = ScriptBuilder::new();
    
    script_builder.add_data(contract_address.as_bytes())?;
    script_builder.add_data(function_data)?;
    script_builder.add_op(0xc1)?;
    
    let script_public_key = script_builder.drain();
    
    let input = TransactionInput {
        previous_outpoint: caller_utxo,
        signature_script: vec![],
        sequence: 0,
        sig_op_count: 0,
    };
    
    let output = TransactionOutput {
        value: caller_amount - fee,
        script_public_key,
    };
    
    Ok(Transaction::new(
        0,
        vec![input],
        vec![output],
        0,
        Hash::from_u64_word(0),
        0,
        vec![],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contract_deployment_transaction() {
        let contract_code = b"simple wasm contract code";
        let deployer_utxo = TransactionOutpoint::new(Hash::from_u64_word(1), 0);
        let deployer_amount = 1000000;
        let fee = 1000;
        
        let tx = create_contract_deployment_transaction(
            contract_code,
            deployer_utxo,
            deployer_amount,
            fee,
        ).unwrap();
        
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, deployer_amount - fee);
    }
    
    #[test]
    fn test_contract_call_transaction() {
        let contract_address = Hash::from_u64_word(12345);
        let function_data = b"transfer(alice, 100)";
        let caller_utxo = TransactionOutpoint::new(Hash::from_u64_word(2), 0);
        let caller_amount = 500000;
        let fee = 500;
        
        let tx = create_contract_call_transaction(
            contract_address,
            function_data,
            caller_utxo,
            caller_amount,
            fee,
        ).unwrap();
        
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, caller_amount - fee);
    }
}
}
