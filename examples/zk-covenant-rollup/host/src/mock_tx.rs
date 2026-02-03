use zk_covenant_rollup_core::{
    action::{Action, VersionedActionRaw},
    is_action_tx_id, payload_digest, tx_id_v1, ACTION_TX_ID_PREFIX,
};

/// Represents a mock transaction to be included in a block
#[derive(Clone, Debug)]
pub enum MockTx {
    /// Version 0 tx: just a raw tx_id (no payload processing)
    V0 { tx_id: [u32; 8] },
    /// Version 1+ tx: has payload and rest_digest
    V1 { version: u16, payload: VersionedActionRaw, rest_digest: [u32; 8] },
}

impl MockTx {
    pub fn version(&self) -> u16 {
        match self {
            MockTx::V0 { .. } => 0,
            MockTx::V1 { version, .. } => *version,
        }
    }

    pub fn tx_id(&self) -> [u32; 8] {
        match self {
            MockTx::V0 { tx_id } => *tx_id,
            MockTx::V1 { payload, rest_digest, .. } => {
                let pd = payload_digest(payload.as_words());
                tx_id_v1(&pd, rest_digest)
            }
        }
    }

    pub fn is_valid_action(&self) -> bool {
        match self {
            MockTx::V0 { .. } => false,
            MockTx::V1 { payload, .. } => {
                is_action_tx_id(&self.tx_id()) && Action::try_from(payload.action_raw).is_ok()
            }
        }
    }

    /// Write to executor env in the format expected by guest
    pub fn write_to_env(&self, builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>) {
        builder.write_slice(&(self.version() as u32).to_le_bytes());
        match self {
            MockTx::V0 { tx_id } => {
                builder.write_slice(bytemuck::cast_slice::<_, u8>(tx_id));
            }
            MockTx::V1 { payload, rest_digest, .. } => {
                builder.write_slice(bytemuck::cast_slice::<_, u8>(payload.as_words()));
                builder.write_slice(bytemuck::cast_slice::<_, u8>(rest_digest));
            }
        }
    }
}

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (single byte)
fn find_action_tx_nonce(action: Action, action_version: u16, rest_digest: [u32; 8]) -> VersionedActionRaw {
    let (discriminator, value) = action.split();
    for nonce in 0u32.. {
        let payload = VersionedActionRaw { action_version, action_raw: [discriminator, value], nonce };
        let tx_id = tx_id_v1(&payload_digest(payload.as_words()), &rest_digest);
        if is_action_tx_id(&tx_id) {
            println!("  Found valid action nonce: {}", nonce);
            return payload;
        }
    }
    unreachable!()
}

/// Create mock transactions for a block demonstrating different tx types
pub fn create_mock_block_txs(block_index: u32) -> Vec<MockTx> {
    let mut txs = Vec::new();

    // Type 1: Regular tx (v0, non-action prefix)
    txs.push(MockTx::V0 {
        tx_id: [0xDEADBEEF, block_index, 0x11111111, 0, 0, 0, 0, 0],
    });

    // Type 2a: v0 tx with action prefix (not processed - guest only checks v1+)
    txs.push(MockTx::V0 {
        tx_id: [ACTION_TX_ID_PREFIX as u32, block_index, 0x22222222, 0, 0, 0, 0, 0],
    });

    // Type 2b: v1 tx with action prefix but invalid discriminator
    let invalid_rest = [block_index, 0x33333333, 0, 0, 0, 0, 0, 0];
    for nonce in 0u32.. {
        let payload = VersionedActionRaw { action_version: 1, action_raw: [255, 0], nonce };
        if is_action_tx_id(&tx_id_v1(&payload_digest(payload.as_words()), &invalid_rest)) {
            txs.push(MockTx::V1 { version: 1, payload, rest_digest: invalid_rest });
            break;
        }
    }

    // Type 3: Valid action tx
    let action = match block_index % 3 {
        0 => Action::Fib(10),
        1 => Action::Factorial(5),
        _ => Action::Fib(15),
    };
    let rest_digest = [block_index, 0x44444444, 0, 0, 0, 0, 0, 0];
    let payload = find_action_tx_nonce(action, 1, rest_digest);
    txs.push(MockTx::V1 { version: 1, payload, rest_digest });

    println!("Block {}: {} txs (1 regular, 1 fake-action, 1 invalid-action, 1 valid {:?})",
             block_index, txs.len(), action);
    txs
}
