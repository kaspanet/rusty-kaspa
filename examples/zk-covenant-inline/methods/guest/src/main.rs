// #![no_std]
// #![no_main]
//
// extern crate alloc;
//
// use alloc::vec;
// use alloc::vec::Vec;
// risc0_zkvm::guest::entry!(main);

use risc0_zkvm::guest::env;
use zk_covenant_inline_core::tx::hashing::id;
use zk_covenant_inline_core::tx::Transaction;
// use zk_covenant_inline_core::{GENESIS_TX_ID};

// use borsh::{BorshDeserialize, BorshSerialize};
// #[derive(Debug, Clone, PartialEq, Eq, Default, BorshSerialize, BorshDeserialize)]
// pub struct Transaction;
// impl Transaction {
//     pub fn id(&self) -> [u8;32] {
//         [0;32]
//     }
// }



pub fn main() {
    let mut curr_tx_bytes_len = [0u32];
    env::read_slice(&mut curr_tx_bytes_len);
    let mut curr_tx_bytes = vec![0u8; curr_tx_bytes_len[0] as usize];
    env::read_slice(&mut curr_tx_bytes);
    let curr_tx: Transaction = borsh::from_slice(curr_tx_bytes.as_slice()).expect("Deserialize failed");
    env::commit_slice(id(&curr_tx).as_bytes());
    // let parent_txid_preimage: Vec<u8> = env::read();
    // let grandparent_txid_preimage: Vec<u8> = env::read();
    // let increment: u64 = env::read();
    // let old_counter = 0u64; // todo remove me

    // let parent_tx: SimpleTx = bincode::deserialize(&parent_txid_preimage).expect("Deserialize failed");
    // let grandparent_tx: SimpleTx = bincode::deserialize(&grandparent_txid_preimage).expect("Deserialize failed");
    //
    // let grandparent_hash = tx_hash(&grandparent_txid_preimage);
    // assert_eq!(grandparent_hash, parent_tx.inputs[0].previous_outpoint_txid, "Chain broken");
    //
    // if grandparent_hash == GENESIS_TX_ID {
    //     // Bootstrap OK.
    // } else {
    //     let parent_spk = &parent_tx.outputs[0].script_public_key;
    //     let grandparent_spk = &grandparent_tx.outputs[0].script_public_key;
    //     assert_eq!(parent_spk, grandparent_spk, "Script mismatch");
    // }
    //
    // let old_counter = u64::from_le_bytes(parent_tx.payload.try_into().expect("Invalid payload"));
    // let new_counter = old_counter + increment;
    //
    // env::commit(&new_counter.to_le_bytes());


}