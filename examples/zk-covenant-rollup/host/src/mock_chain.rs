use std::collections::HashMap;

use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutput};
use kaspa_hashes::{Hash, SeqCommitmentMerkleBranchHash};
use kaspa_txscript::seq_commit_accessor::SeqCommitAccessor;
use zk_covenant_rollup_core::{
    pay_to_pubkey_spk,
    seq_commit::seq_commitment_leaf,
    smt::Smt,
    state::{AccountWitness, StateRoot},
};

use crate::mock_tx::{
    create_prev_tx, create_transfer_tx, create_unknown_action_tx, create_v0_tx, create_v1_non_action_tx, ZkTransaction,
};

/// Mock implementation of SeqCommitAccessor for testing
pub struct MockSeqCommitAccessor(pub HashMap<Hash, Hash>);

impl SeqCommitAccessor for MockSeqCommitAccessor {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool> {
        self.0.contains_key(&block_hash).then_some(true)
    }

    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash> {
        self.0.get(&block_hash).copied()
    }
}

/// Named accounts for the demo
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccountName {
    Alice,
    Bob,
    Charlie,
    Dave,
}

impl AccountName {
    /// Get the pubkey for this account (deterministic based on name) as [u32; 8]
    /// Layout: first byte = key index, then name bytes in little-endian u32 words
    pub fn pubkey(&self) -> [u32; 8] {
        match self {
            // key=10, "alice" = [0x61, 0x6c, 0x69, 0x63, 0x65]
            AccountName::Alice => {
                let w0 = 10u32 | (b'a' as u32) << 8 | (b'l' as u32) << 16 | (b'i' as u32) << 24;
                let w1 = (b'c' as u32) | (b'e' as u32) << 8;
                [w0, w1, 0, 0, 0, 0, 0, 0]
            }
            // key=20, "bob"
            AccountName::Bob => {
                let w0 = 20u32 | (b'b' as u32) << 8 | (b'o' as u32) << 16 | (b'b' as u32) << 24;
                [w0, 0, 0, 0, 0, 0, 0, 0]
            }
            // key=30, "charlie"
            AccountName::Charlie => {
                let w0 = 30u32 | (b'c' as u32) << 8 | (b'h' as u32) << 16 | (b'a' as u32) << 24;
                let w1 = (b'r' as u32) | (b'l' as u32) << 8 | (b'i' as u32) << 16 | (b'e' as u32) << 24;
                [w0, w1, 0, 0, 0, 0, 0, 0]
            }
            // key=40, "dave"
            AccountName::Dave => {
                let w0 = 40u32 | (b'd' as u32) << 8 | (b'a' as u32) << 16 | (b'v' as u32) << 24;
                let w1 = b'e' as u32;
                [w0, w1, 0, 0, 0, 0, 0, 0]
            }
        }
    }

    /// Get the pubkey as bytes (for SPK creation)
    pub fn pubkey_bytes(&self) -> [u8; 32] {
        bytemuck::cast(self.pubkey())
    }

    pub fn name(&self) -> &'static str {
        match self {
            AccountName::Alice => "Alice",
            AccountName::Bob => "Bob",
            AccountName::Charlie => "Charlie",
            AccountName::Dave => "Dave",
        }
    }
}

/// A transfer operation for the demo
#[derive(Clone, Debug)]
pub struct Transfer {
    pub from: AccountName,
    pub to: AccountName,
    pub amount: u64,
}

impl Transfer {
    pub fn new(from: AccountName, to: AccountName, amount: u64) -> Self {
        Self { from, to, amount }
    }
}

/// Result of building a mock chain
pub struct MockChain {
    pub block_hashes: Vec<Hash>,
    pub block_txs: Vec<Vec<ZkTransaction>>,
    pub accessor: MockSeqCommitAccessor,
    pub final_seq_commit: Hash,
    pub final_state_root: StateRoot,
}

/// Build a mock chain with account-based transfers
pub fn build_mock_chain(initial_seq_commit: Hash) -> MockChain {
    // Initialize accounts
    let mut smt = Smt::new();
    let mut balances: HashMap<AccountName, u64> = HashMap::new();

    // Initial balances
    println!("\n=== Initial Accounts ===");
    for (account, balance) in [(AccountName::Alice, 1000u64), (AccountName::Bob, 500), (AccountName::Charlie, 0)] {
        smt.insert(account.pubkey(), balance);
        balances.insert(account, balance);
        println!("  {}: {} tokens", account.name(), balance);
    }

    let initial_root = smt.root();
    println!("\nInitial state root: {}", faster_hex::hex_string(bytemuck::bytes_of(&initial_root)));

    // Define the transfer scenario
    let blocks = [
        // Block 1
        vec![Transfer::new(AccountName::Alice, AccountName::Bob, 100), Transfer::new(AccountName::Bob, AccountName::Charlie, 50)],
        // Block 2
        vec![
            Transfer::new(AccountName::Charlie, AccountName::Alice, 25),
            Transfer::new(AccountName::Alice, AccountName::Dave, 500), // Creates new account
        ],
        // Block 3
        vec![
            Transfer::new(AccountName::Bob, AccountName::Alice, 1000), // Invalid - insufficient balance
            Transfer::new(AccountName::Alice, AccountName::Bob, 200),
        ],
    ];

    let mut block_hashes: Vec<Hash> = Vec::new();
    let mut block_txs: Vec<Vec<ZkTransaction>> = Vec::new();
    let mut accessor_map = HashMap::new();
    let mut seq_commit = initial_seq_commit;

    for (block_idx, transfers) in blocks.iter().enumerate() {
        println!("\n=== Processing Block {} ===", block_idx + 1);
        block_hashes.push(Hash::from_u64_word((block_idx + 1) as u64));

        let mut txs = Vec::new();

        // Add a regular V0 tx first
        txs.push(create_v0_tx([0xDEADBEEF, block_idx as u32, 0, 0, 0, 0, 0, 0]));

        // Block 1: Add a V1 non-action tx (should be ignored by guest)
        if block_idx == 0 {
            println!("  Adding V1 non-action tx (should be ignored)");
            txs.push(create_v1_non_action_tx());
        }

        // Block 2: Add an unknown action tx (has action prefix but unknown op code)
        if block_idx == 1 {
            println!("  Adding unknown action tx (should be ignored)");
            txs.push(create_unknown_action_tx());
        }

        for transfer in transfers.iter() {
            let from_pk = transfer.from.pubkey();
            let to_pk = transfer.to.pubkey();
            let from_balance = *balances.get(&transfer.from).unwrap_or(&0);
            let to_balance = *balances.get(&transfer.to).unwrap_or(&0);

            // Check if transfer is valid
            let is_valid = from_balance >= transfer.amount;
            let status = if is_valid { "✓" } else { "✗ (insufficient balance)" };
            println!("  {} → {}: {} tokens {}", transfer.from.name(), transfer.to.name(), transfer.amount, status);

            if is_valid {
                // Generate source proof from current SMT state
                let source_proof = smt.prove(&from_pk);

                // Create source witness
                let source_witness = AccountWitness::new(from_pk, from_balance, source_proof);

                // Check if dest account exists before update
                let dest_exists = smt.get(&to_pk).is_some();

                // Update source balance first to create intermediate state
                let new_from_balance = from_balance - transfer.amount;
                smt.upsert(from_pk, new_from_balance);

                // Now generate dest proof against intermediate state (after source update)
                let dest_proof = smt.prove(&to_pk);

                // Create dest witness - check if account exists
                let dest_witness = if dest_exists {
                    AccountWitness::new(to_pk, to_balance, dest_proof)
                } else {
                    // New account - provide empty witness
                    AccountWitness::new([0u32; 8], 0, dest_proof)
                };

                // Create SPK for the source account (proves source owns the account)
                let first_input_spk = pay_to_pubkey_spk(&transfer.from.pubkey_bytes());
                let first_input_spk_kaspa = ScriptPublicKey::new(0, first_input_spk.to_vec().into());

                // Create a "previous transaction" that has the output with source's SPK
                // This simulates the UTXO being spent
                let prev_tx = create_prev_tx(1000, first_input_spk_kaspa);

                // Create mock outputs for the transfer transaction
                // In a real scenario, these would be the actual outputs
                let outputs = vec![TransactionOutput::new(
                    transfer.amount,
                    ScriptPublicKey::new(0, pay_to_pubkey_spk(&transfer.to.pubkey_bytes()).to_vec().into()),
                )];

                // Create the transfer transaction
                let tx = create_transfer_tx(from_pk, to_pk, transfer.amount, outputs, source_witness, dest_witness, prev_tx, 0);

                txs.push(tx);

                // Update dest balance to complete the transfer
                let new_to_balance = to_balance + transfer.amount;
                smt.upsert(to_pk, new_to_balance);

                // Update balances tracking
                balances.insert(transfer.from, new_from_balance);
                balances.insert(transfer.to, new_to_balance);

                // Insert new account if it didn't exist
                if !dest_exists {
                    println!("    (new account created for {})", transfer.to.name());
                }
            } else {
                // Skip invalid transfers - don't add to txs
                println!("    (transfer skipped)");
            }
        }

        // Compute tx leaf digests for seq_commitment
        let tx_digests: Vec<Hash> = txs
            .iter()
            .map(|tx| {
                let leaf = seq_commitment_leaf(&tx.tx_id(), tx.version());
                Hash::from_bytes(bytemuck::cast_slice(&leaf).try_into().unwrap())
            })
            .collect();

        // Update seq_commitment
        seq_commit = calc_accepted_id_merkle_root(seq_commit, tx_digests.into_iter());
        accessor_map.insert(block_hashes[block_idx], seq_commit);

        block_txs.push(txs);
    }

    // Print final balances
    println!("\n=== Final Accounts ===");
    for account in [AccountName::Alice, AccountName::Bob, AccountName::Charlie, AccountName::Dave] {
        if let Some(balance) = balances.get(&account) {
            println!("  {}: {} tokens", account.name(), balance);
        }
    }

    let final_root = smt.root();
    println!("\nFinal state root: {}", faster_hex::hex_string(bytemuck::bytes_of(&final_root)));

    MockChain {
        block_hashes,
        block_txs,
        accessor: MockSeqCommitAccessor(accessor_map),
        final_seq_commit: seq_commit,
        final_state_root: final_root,
    }
}

/// Get the initial SMT for the demo scenario
pub fn build_initial_smt() -> Smt {
    let mut smt = Smt::new();
    smt.insert(AccountName::Alice.pubkey(), 1000);
    smt.insert(AccountName::Bob.pubkey(), 500);
    smt.insert(AccountName::Charlie.pubkey(), 0);
    smt
}

pub fn calc_accepted_id_merkle_root(prev: Hash, tx_digests: impl ExactSizeIterator<Item = Hash>) -> Hash {
    kaspa_merkle::merkle_hash_with_hasher(
        prev,
        kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(tx_digests),
        SeqCommitmentMerkleBranchHash::new(),
    )
}

pub fn from_bytes(arr: [u8; 32]) -> [u32; 8] {
    let mut out = [0; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(&arr);
    out
}
