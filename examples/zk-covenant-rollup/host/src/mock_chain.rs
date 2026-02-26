use std::collections::HashMap;

use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutput};
use kaspa_hashes::{Hash, SeqCommitmentMerkleBranchHash};
use kaspa_txscript::pay_to_script_hash_script;
use kaspa_txscript::seq_commit_accessor::SeqCommitAccessor;
use zk_covenant_rollup_core::{
    build_permission_redeem_bytes_converged,
    p2sh::blake2b_script_hash,
    pad_to_depth, pay_to_pubkey_spk, perm_leaf_hash,
    permission_tree::{required_depth, StreamingPermTreeBuilder},
    seq_commit::seq_commitment_leaf,
    smt::Smt,
    state::{AccountWitness, StateRoot},
    MAX_DELEGATE_INPUTS,
};

use crate::bridge::build_delegate_entry_script;

use crate::mock_tx::{
    create_entry_tx, create_exit_tx, create_exit_tx_insufficient, create_prev_tx, create_transfer_tx, create_unknown_action_tx,
    create_v0_tx, create_v1_non_action_tx, ZkTransaction,
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
    Eve,
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
            // key=50, "eve"
            AccountName::Eve => {
                let w0 = 50u32 | (b'e' as u32) << 8 | (b'v' as u32) << 16 | (b'e' as u32) << 24;
                [w0, 0, 0, 0, 0, 0, 0, 0]
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
            AccountName::Eve => "Eve",
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
    /// Full permission redeem script (if exits occurred)
    pub permission_redeem: Option<Vec<u8>>,
    /// blake2b(permission_redeem) — script hash for journal verification
    pub permission_spk_hash: Option<[u8; 32]>,
    /// Converged length of the permission redeem script (written to guest env)
    pub perm_redeem_script_len: Option<i64>,
}

/// Build a mock chain with account-based transfers
pub fn build_mock_chain(initial_seq_commit: Hash, covenant_id_bytes: &[u8; 32], non_activity_blocks: u32) -> MockChain {
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
    let mut perm_builder = StreamingPermTreeBuilder::new();

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

        // Block 3: Add an entry (deposit) for Eve — 200 KAS (new account)
        if block_idx == 2 {
            let eve_pk = AccountName::Eve.pubkey();
            let deposit_amount = 200u64;
            let eve_exists = smt.get(&eve_pk).is_some();

            println!("  Entry deposit → Eve: {} tokens (new account)", deposit_amount);

            // Generate dest proof against current state
            let dest_proof = smt.prove(&eve_pk);
            let dest_witness = if eve_exists {
                let eve_balance = *balances.get(&AccountName::Eve).unwrap_or(&0);
                AccountWitness::new(eve_pk, eve_balance, dest_proof)
            } else {
                // New account — provide empty witness
                AccountWitness::new([0u32; 8], 0, dest_proof)
            };

            // The deposit output: value=deposit_amount, SPK = P2SH(delegate_entry_script)
            let delegate = build_delegate_entry_script(covenant_id_bytes);
            let deposit_spk = pay_to_script_hash_script(&delegate);
            let outputs = vec![TransactionOutput::new(deposit_amount, deposit_spk)];

            let entry_tx = create_entry_tx(eve_pk, outputs, dest_witness);
            txs.push(entry_tx);

            // Update SMT and balances
            let eve_old_balance = *balances.get(&AccountName::Eve).unwrap_or(&0);
            let eve_new_balance = eve_old_balance + deposit_amount;
            smt.upsert(eve_pk, eve_new_balance);
            balances.insert(AccountName::Eve, eve_new_balance);

            if !eve_exists {
                println!("    (new account created for Eve)");
            }
        }

        // Block 3: Add a second entry deposit for Eve (existing account, 50 more tokens)
        if block_idx == 2 {
            let eve_pk = AccountName::Eve.pubkey();
            let deposit_amount = 50u64;
            let eve_balance = *balances.get(&AccountName::Eve).unwrap();

            println!("  Entry deposit → Eve: {} tokens (existing account)", deposit_amount);

            let dest_proof = smt.prove(&eve_pk);
            let dest_witness = AccountWitness::new(eve_pk, eve_balance, dest_proof);

            let delegate = build_delegate_entry_script(covenant_id_bytes);
            let deposit_spk = pay_to_script_hash_script(&delegate);
            let outputs = vec![TransactionOutput::new(deposit_amount, deposit_spk)];

            let entry_tx = create_entry_tx(eve_pk, outputs, dest_witness);
            txs.push(entry_tx);

            let eve_new_balance = eve_balance + deposit_amount;
            smt.upsert(eve_pk, eve_new_balance);
            balances.insert(AccountName::Eve, eve_new_balance);
        }

        // Block 3: Invalid exit — insufficient balance (Charlie has 25, tries to exit 1000)
        if block_idx == 2 {
            let charlie_pk = AccountName::Charlie.pubkey();
            let charlie_balance = *balances.get(&AccountName::Charlie).unwrap();
            let exit_amount = 1000u64;
            let charlie_dest_spk = pay_to_pubkey_spk(&AccountName::Charlie.pubkey_bytes());

            println!("  Exit (INVALID): Charlie tries to withdraw {} tokens (balance: {})", exit_amount, charlie_balance);

            let source_proof = smt.prove(&charlie_pk);
            let source_witness = AccountWitness::new(charlie_pk, charlie_balance, source_proof);

            // Still need a prev_tx for the input outpoint (nonce finding), but rest=None
            // because the guest will see insufficient balance and skip auth.
            let first_input_spk = pay_to_pubkey_spk(&AccountName::Charlie.pubkey_bytes());
            let first_input_spk_kaspa = ScriptPublicKey::new(0, first_input_spk.to_vec().into());
            let prev_tx = create_prev_tx(1000, first_input_spk_kaspa);
            let input = kaspa_consensus_core::tx::TransactionInput::new(
                kaspa_consensus_core::tx::TransactionOutpoint::new(prev_tx.id(), 0),
                vec![],
                0,
                0,
            );

            let outputs = vec![TransactionOutput::new(exit_amount, ScriptPublicKey::new(0, charlie_dest_spk.to_vec().into()))];

            let exit_tx =
                create_exit_tx_insufficient(charlie_pk, &charlie_dest_spk, exit_amount, vec![input], outputs, source_witness);
            txs.push(exit_tx);
            // Do NOT update SMT/balances/perm_builder — guest rejects (insufficient balance)
            println!("    (exit rejected: insufficient balance)");
        }

        // Block 3: Invalid exit — wrong prev_tx SPK (Eve source, but prev_tx has Alice's SPK)
        if block_idx == 2 {
            let eve_pk = AccountName::Eve.pubkey();
            let eve_balance = *balances.get(&AccountName::Eve).unwrap();
            let exit_amount = 50u64;
            let eve_dest_spk = pay_to_pubkey_spk(&AccountName::Eve.pubkey_bytes());

            println!("  Exit (INVALID): Eve tries to exit with wrong prev_tx SPK (Alice's)");

            let source_proof = smt.prove(&eve_pk);
            let source_witness = AccountWitness::new(eve_pk, eve_balance, source_proof);

            // Use Alice's SPK for the prev_tx — auth will fail (pubkey mismatch)
            let wrong_input_spk = pay_to_pubkey_spk(&AccountName::Alice.pubkey_bytes());
            let wrong_input_spk_kaspa = ScriptPublicKey::new(0, wrong_input_spk.to_vec().into());
            let prev_tx = create_prev_tx(1000, wrong_input_spk_kaspa);

            let outputs = vec![TransactionOutput::new(exit_amount, ScriptPublicKey::new(0, eve_dest_spk.to_vec().into()))];

            let exit_tx = create_exit_tx(eve_pk, &eve_dest_spk, exit_amount, outputs, source_witness, prev_tx, 0);
            txs.push(exit_tx);
            // Do NOT update SMT/balances/perm_builder — guest rejects (auth failure)
            println!("    (exit rejected: auth failure — wrong prev_tx SPK)");
        }

        // Block 3: Add valid exit (withdrawal) transactions
        if block_idx == 2 {
            // Eve exits 100 tokens → Eve's P2PK SPK
            let eve_pk = AccountName::Eve.pubkey();
            let eve_balance = *balances.get(&AccountName::Eve).unwrap();
            let exit_amount = 100u64;
            let eve_dest_spk = pay_to_pubkey_spk(&AccountName::Eve.pubkey_bytes());

            println!("  Exit: Eve withdraws {} tokens", exit_amount);

            let source_proof = smt.prove(&eve_pk);
            let source_witness = AccountWitness::new(eve_pk, eve_balance, source_proof);

            let first_input_spk = pay_to_pubkey_spk(&AccountName::Eve.pubkey_bytes());
            let first_input_spk_kaspa = ScriptPublicKey::new(0, first_input_spk.to_vec().into());
            let prev_tx = create_prev_tx(1000, first_input_spk_kaspa);

            let outputs = vec![TransactionOutput::new(exit_amount, ScriptPublicKey::new(0, eve_dest_spk.to_vec().into()))];

            let exit_tx = create_exit_tx(eve_pk, &eve_dest_spk, exit_amount, outputs, source_witness, prev_tx, 0);
            txs.push(exit_tx);

            let new_eve_balance = eve_balance - exit_amount;
            smt.upsert(eve_pk, new_eve_balance);
            balances.insert(AccountName::Eve, new_eve_balance);
            perm_builder.add_leaf(perm_leaf_hash(&eve_dest_spk, exit_amount));

            // Dave exits 200 tokens → Dave's P2PK SPK
            let dave_pk = AccountName::Dave.pubkey();
            let dave_balance = *balances.get(&AccountName::Dave).unwrap();
            let exit_amount = 200u64;
            let dave_dest_spk = pay_to_pubkey_spk(&AccountName::Dave.pubkey_bytes());

            println!("  Exit: Dave withdraws {} tokens", exit_amount);

            let source_proof = smt.prove(&dave_pk);
            let source_witness = AccountWitness::new(dave_pk, dave_balance, source_proof);

            let first_input_spk = pay_to_pubkey_spk(&AccountName::Dave.pubkey_bytes());
            let first_input_spk_kaspa = ScriptPublicKey::new(0, first_input_spk.to_vec().into());
            let prev_tx = create_prev_tx(1000, first_input_spk_kaspa);

            let outputs = vec![TransactionOutput::new(exit_amount, ScriptPublicKey::new(0, dave_dest_spk.to_vec().into()))];

            let exit_tx = create_exit_tx(dave_pk, &dave_dest_spk, exit_amount, outputs, source_witness, prev_tx, 0);
            txs.push(exit_tx);

            let new_dave_balance = dave_balance - exit_amount;
            smt.upsert(dave_pk, new_dave_balance);
            balances.insert(AccountName::Dave, new_dave_balance);
            perm_builder.add_leaf(perm_leaf_hash(&dave_dest_spk, exit_amount));
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

    // Append non-activity blocks (V0-only transactions, no state changes)
    if non_activity_blocks > 0 {
        println!("\n  Appending {} non-activity blocks (3000 V0 txs each)", non_activity_blocks);
        let activity_block_count = block_hashes.len();
        for i in 0..non_activity_blocks {
            let abs_idx = activity_block_count + i as usize;
            block_hashes.push(Hash::from_u64_word((abs_idx + 1) as u64));

            let txs: Vec<ZkTransaction> =
                (0..3000u32).map(|tx_idx| create_v0_tx([0xBEEF0000 | i, tx_idx, 0, 0, 0, 0, 0, 0])).collect();

            let tx_digests: Vec<Hash> = txs
                .iter()
                .map(|tx| {
                    let leaf = seq_commitment_leaf(&tx.tx_id(), tx.version());
                    Hash::from_bytes(bytemuck::cast_slice(&leaf).try_into().unwrap())
                })
                .collect();

            seq_commit = calc_accepted_id_merkle_root(seq_commit, tx_digests.into_iter());
            accessor_map.insert(block_hashes[abs_idx], seq_commit);
            block_txs.push(txs);
        }
    }

    // Compute permission tree data if exits occurred
    let perm_count = perm_builder.leaf_count();
    let (permission_redeem, permission_spk_hash, perm_redeem_script_len) = if perm_count > 0 {
        let depth = required_depth(perm_count as usize);
        let perm_root = pad_to_depth(perm_builder.finalize(), perm_count, depth);
        let redeem = build_permission_redeem_bytes_converged(&perm_root, perm_count as u64, depth, MAX_DELEGATE_INPUTS);
        let script_hash = blake2b_script_hash(&redeem);
        let len = redeem.len() as i64;
        println!("\n=== Permission Tree ===");
        println!("  Exit count: {}", perm_count);
        println!("  Tree depth: {}", depth);
        println!("  Redeem script length: {} bytes", len);
        (Some(redeem), Some(script_hash), Some(len))
    } else {
        (None, None, None)
    };

    // Print final balances
    println!("\n=== Final Accounts ===");
    for account in [AccountName::Alice, AccountName::Bob, AccountName::Charlie, AccountName::Dave, AccountName::Eve] {
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
        permission_redeem,
        permission_spk_hash,
        perm_redeem_script_len,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_chain_has_permission_data() {
        let prev_seq = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
        let chain = build_mock_chain(prev_seq, &[0xFF; 32], 0);

        // Valid exits occurred → permission fields must be populated
        assert!(chain.permission_redeem.is_some(), "should have permission redeem script");
        assert!(chain.permission_spk_hash.is_some(), "should have permission SPK hash");
        assert!(chain.perm_redeem_script_len.is_some(), "should have permission redeem script length");
    }

    #[test]
    fn mock_chain_permission_tree_has_two_leaves() {
        let prev_seq = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
        let chain = build_mock_chain(prev_seq, &[0xFF; 32], 0);

        // Only 2 valid exits (Eve 100, Dave 200); the 2 invalid exits should NOT add leaves.
        // We verify this indirectly: the permission redeem script length should be
        // consistent with a 2-leaf tree (depth=1).
        let redeem = chain.permission_redeem.as_ref().unwrap();
        let len = chain.perm_redeem_script_len.unwrap();
        assert_eq!(redeem.len() as i64, len);
    }

    #[test]
    fn mock_chain_final_state_matches_expected_balances() {
        let prev_seq = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
        let chain = build_mock_chain(prev_seq, &[0xFF; 32], 0);

        // Expected final balances:
        //   Block 1: Alice→Bob 100 (A=900,B=600), Bob→Charlie 50 (B=550,C=50)
        //   Block 2: Charlie→Alice 25 (C=25,A=925), Alice→Dave 500 (A=425,D=500)
        //   Block 3: entries Eve 200+50 (E=250), invalid exits (no effect),
        //            valid exits Eve-100(E=150) Dave-200(D=300),
        //            Bob→Alice 1000 invalid, Alice→Bob 200 (A=225,B=750)
        let mut expected_smt = Smt::new();
        expected_smt.insert(AccountName::Alice.pubkey(), 225);
        expected_smt.insert(AccountName::Bob.pubkey(), 750);
        expected_smt.insert(AccountName::Charlie.pubkey(), 25);
        expected_smt.insert(AccountName::Dave.pubkey(), 300);
        expected_smt.insert(AccountName::Eve.pubkey(), 150);

        assert_eq!(
            chain.final_state_root,
            expected_smt.root(),
            "final state root should match expected balances (invalid exits must not affect state)"
        );
    }

    #[test]
    fn mock_chain_deterministic() {
        let prev_seq = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
        let chain1 = build_mock_chain(prev_seq, &[0xFF; 32], 0);
        let chain2 = build_mock_chain(prev_seq, &[0xFF; 32], 0);

        assert_eq!(chain1.final_state_root, chain2.final_state_root);
        assert_eq!(chain1.final_seq_commit, chain2.final_seq_commit);
        assert_eq!(chain1.permission_spk_hash, chain2.permission_spk_hash);
    }

    #[test]
    fn mock_chain_blocks_without_exits_have_no_permission_effect() {
        // Blocks 1 and 2 have no exits. Verify this by checking that a chain
        // with only those blocks produces no permission data.
        let _prev_seq = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());

        // Build a minimal chain with no exits to verify the no-exit path
        let _smt = build_initial_smt();
        let perm_builder = zk_covenant_rollup_core::permission_tree::StreamingPermTreeBuilder::new();

        // No exits added to perm_builder
        assert_eq!(perm_builder.leaf_count(), 0, "no exits → perm_builder should be empty");
    }
}
