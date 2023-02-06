use std::sync::Arc;

use crate::{RpcUtxoEntry, RpcUtxosByAddressesEntry, UtxosChangedNotification};
use addresses::Prefix;
use utxoindex::notify::UtxoChangesNotification;

//TODO: more work when txscript, this also needs access to network type for the prefix to do address conversion (currently this is hardcoded in [`ConsensusCollectorNotifiy`])

///This converts a raw utxoindex emmitted [`UtxoChangesNotification`], together with a [`Prefix`] into the rpc-friendly [`UtxosChangedNotification`] version.
impl TryFrom<(UtxoChangesNotification, Prefix)> for UtxosChangedNotification {
    fn try_from(item: (UtxoChangesNotification, Prefix)) -> Result<Self, E> {
        Ok(Self {
            added: Vec::<Arc<RpcUtxosByAddressesEntry>>::from_iter(item.0.added.into_iter().flat_map(
                move |(script_public_key, compact_utxo_collection)| {
                    compact_utxo_collection.into_iter().map(move |(transaction_outpoint, compact_utxo)| {
                        Arc::new(RpcUtxosByAddressesEntry {
                            address: Address {
                                //we assume schnorr p2pk for integration / testing.
                                prefix: item.1,
                                payload: script_public_key.script()[1..33].to_vec(), //slice out OPPush at start and OPCHECKSIG at the end.
                                version: script_public_key.version() as u8,
                            }, //TODO: use proper txscript for this, when available.
                            transaction_outpoint,
                            utxo_entry: RpcUtxoEntry {
                                amount: compact_utxo.amount,
                                script_public_key,
                                block_daa_score: compact_utxo.block_daa_score,
                                is_coinbase: compact_utxo.is_coinbase,
                            },
                        })
                    });
                },
            )),

            removed: Vec::<Arc<RpcUtxosByAddressesEntry>>::from_iter(item.0.removed.into_iter().flat_map(
                move |(script_public_key, compact_utxo_collection)| {
                    compact_utxo_collection.into_iter().map(move |(transaction_outpoint, compact_utxo)| {
                        Arc::new(RpcUtxosByAddressesEntry {
                            address: Address {
                                //we assume schnorr p2pk for integration / testing.
                                prefix: item.1,
                                payload: script_public_key.script()[1..33].to_vec(),
                                version: script_public_key.version() as u8,
                            }, //TODO: use proper txscript for this, when available., //TODO: use proper txscript for this
                            transaction_outpoint,
                            utxo_entry: RpcUtxoEntry {
                                amount: compact_utxo.amount,
                                script_public_key,
                                block_daa_score: compact_utxo.block_daa_score,
                                is_coinbase: compact_utxo.is_coinbase,
                            },
                        });
                    })
                },
            )),
        })
    }
}

//TODO: write Test
