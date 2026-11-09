use crate::{AddressManager, MAX_ADDRESSES, NetAddress};
use ed25519_dalek::{Signer, SigningKey};
use kaspa_consensus_core::config::{Config, params::SIMNET_PARAMS};
use kaspa_core::task::tick::TickService;
use kaspa_database::{
    create_temp_db,
    prelude::{CachePolicy, ConnBuilder, DB},
};
use kaspa_utils::connection::{LibP2pAddress, Libp2pMetadata, PeerAddress};
use std::{collections::HashSet, sync::Arc};

fn manager(db: Arc<DB>) -> Arc<parking_lot::Mutex<AddressManager>> {
    let mut config = Config::new(SIMNET_PARAMS);
    config.disable_upnp = true;
    AddressManager::new(Arc::new(config), db, Arc::new(TickService::default())).0
}

fn signed(tcp: &str, seed: u8, nonce: u64) -> PeerAddress {
    let key = SigningKey::from_bytes(&[seed; 32]);
    let mut address = PeerAddress {
        libp2p: Some(Libp2pMetadata {
            endpoint_id: key.verifying_key().to_bytes(),
            relay_port: 16112,
            port: 16112,
            nonce,
            signature: Vec::new(),
        }),
        relay_address: Some(LibP2pAddress { address: "9.9.9.9:16112".parse().unwrap(), id: [3; 32] }),
        ..PeerAddress::from(tcp.parse::<NetAddress>().unwrap())
    };
    let signature = key.sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec();
    address.libp2p.as_mut().unwrap().signature = signature;
    address
}

fn resign(address: &mut PeerAddress, seed: u8) {
    let signature = SigningKey::from_bytes(&[seed; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec();
    address.libp2p.as_mut().unwrap().signature = signature;
}

#[test]
fn signature_covers_every_advertised_field_and_network() {
    let address = signed("8.8.8.8:16111", 1, 1);
    assert!(address.verify("kaspa-simnet"));
    assert!(!address.verify("kaspa-mainnet"));
    for field in 0..11 {
        let mut changed = address.clone();
        match field {
            0 => changed.ip = "1.1.1.1".parse().unwrap(),
            1 => changed.tcp_port += 1,
            2 => changed.libp2p.as_mut().unwrap().relay_port = 42,
            3 => changed.libp2p.as_mut().unwrap().endpoint_id = [2; 32],
            4 => changed.libp2p.as_mut().unwrap().port = 0,
            5 => changed.relay_address = None,
            6 => changed.libp2p.as_mut().unwrap().nonce += 1,
            7 => changed.libp2p.as_mut().unwrap().signature[0] ^= 1,
            8 => changed.relay_address.as_mut().unwrap().id[0] ^= 1,
            9 => changed.relay_address.as_mut().unwrap().address.set_port(42),
            _ => changed.relay_address.as_mut().unwrap().address.set_ip("1.1.1.1".parse().unwrap()),
        }
        assert!(!changed.verify("kaspa-simnet"), "unsigned change to field {field}");
    }
    let legacy = PeerAddress::from("8.8.8.8:16111".parse::<NetAddress>().unwrap());
    assert!(legacy.verify("kaspa-simnet"));
    let mut forged = legacy;
    forged.relay_address = Some(LibP2pAddress { address: "9.9.9.9:16112".parse().unwrap(), id: [3; 32] });
    assert!(!forged.verify("kaspa-simnet"));
}

#[test]
fn only_newer_valid_owner_updates_replace_data_and_preserve_local_failures() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db.clone());
    let mut book = addresses.lock();
    let original = signed("8.8.8.8:16111", 1, 5);
    book.add_address(original.clone());
    book.mark_connection_failure(original.clone());
    book.record_relay_result(&original, false);
    for nonce in [4, 5, 6] {
        let mut changed = original.clone();
        changed.libp2p.as_mut().unwrap().nonce = nonce;
        changed.libp2p.as_mut().unwrap().port = 12345;
        if nonce != 6 {
            resign(&mut changed, 1);
        }
        book.add_address(changed);
        assert_eq!(book.iterator().collect::<Vec<_>>(), vec![original.clone()]);
    }
    let mut fresh = original.clone();
    fresh.libp2p.as_mut().unwrap().nonce += 1;
    fresh.libp2p.as_mut().unwrap().port = 12345;
    fresh.relay_address = None;
    fresh.libp2p.as_mut().unwrap().relay_port = 12346;
    resign(&mut fresh, 1);
    book.add_address(fresh.clone());
    book.add_address(original.clone());
    book.add_address(fresh.clone());
    assert_eq!(book.iterator().collect::<Vec<_>>(), vec![fresh.clone()]);
    let entry = book.address_store.get(&fresh.key()).unwrap();
    assert_eq!(entry.connection_failed_count, 2);
    assert_eq!(entry.consecutive_relay_failures, 2);
    // A result for an outdated relay service must not charge its replacement.
    book.record_relay_result(&original, false);
    assert_eq!(book.address_store.get(&fresh.key()).unwrap().consecutive_relay_failures, 2);
    drop(book);
    drop(addresses);
    assert_eq!(manager(db).lock().iterator().collect::<Vec<_>>(), vec![fresh]);
}

#[test]
fn fresh_addresses_skip_signature_verification_and_keys_separate_identities() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    let mut bad = signed("8.8.8.8:16111", 1, 1);
    bad.tcp_port += 1;
    book.add_address(bad.clone());
    assert!(!book.address_store.get(&bad.key()).unwrap().signature_verified);
    assert_eq!(book.iterator().collect::<Vec<_>>(), vec![bad.clone()]);
    let a = signed("8.8.8.8:16111", 1, 1);
    let b = signed("8.8.8.8:16111", 2, 1);
    let legacy = PeerAddress::from(a.tcp_address());
    for record in [&a, &b, &legacy] {
        book.add_address(record.clone());
    }
    assert_eq!(book.iterator().collect::<HashSet<_>>(), HashSet::from([bad, a, b, legacy]));
}

#[test]
fn signature_verification_cache_tracks_current_advertisement_and_survives_reload() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db.clone());
    let mut book = addresses.lock();
    let original = signed("8.8.8.8:16111", 1, 5);
    book.add_address(original.clone());
    assert!(!book.address_store.get(&original.key()).unwrap().signature_verified);
    // An equal-nonce update verifies the stored record before rejecting the update.
    book.add_address(original.clone());
    assert!(book.address_store.get(&original.key()).unwrap().signature_verified);
    drop(book);
    drop(addresses);
    let addresses = manager(db.clone());
    let mut book = addresses.lock();
    assert!(book.address_store.get(&original.key()).unwrap().signature_verified);
    book.mark_connection_failure(original.clone());
    book.record_relay_result(&original, false);
    assert!(book.address_store.get(&original.key()).unwrap().signature_verified);

    let mut newer = original.clone();
    newer.libp2p.as_mut().unwrap().nonce += 1;
    book.add_address(newer.clone());
    assert_eq!(book.address(original.key()), Some(original.clone()), "cached verification must not bypass the new signature");
    assert!(book.address_store.get(&original.key()).unwrap().signature_verified);
    resign(&mut newer, 1);
    book.add_address(newer.clone());
    assert_eq!(book.address(newer.key()), Some(newer.clone()));
    assert!(book.address_store.get(&newer.key()).unwrap().signature_verified);
    drop(book);
    drop(addresses);

    let addresses = manager(db);
    let mut book = addresses.lock();
    assert!(book.address_store.get(&newer.key()).unwrap().signature_verified);
    book.add_address(original);
    assert_eq!(book.address(newer.key()), Some(newer.clone()));
    assert!(book.address_store.get(&newer.key()).unwrap().signature_verified);
}

#[test]
fn valid_updates_repair_invalid_stored_records_regardless_of_nonce_and_preserve_failures() {
    for nonce in [4, 5, 6] {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let addresses = manager(db.clone());
        let mut invalid = signed("8.8.8.8:16111", 1, 5);
        invalid.libp2p.as_mut().unwrap().port = 12345;
        {
            let mut book = addresses.lock();
            book.add_address(invalid.clone());
            book.mark_connection_failure(invalid.clone());
            book.record_relay_result(&invalid, false);
        }
        drop(addresses);
        let addresses = manager(db.clone());
        let mut book = addresses.lock();
        assert_eq!(book.address(invalid.key()), Some(invalid.clone()));
        let mut forged_update = invalid.clone();
        forged_update.libp2p.as_mut().unwrap().nonce = 100;
        book.add_address(forged_update);
        assert_eq!(book.address(invalid.key()), Some(invalid.clone()), "an invalid signature cannot replace an invalid record");
        let valid = signed("8.8.8.8:16111", 1, nonce);
        book.add_address(valid.clone());
        assert_eq!(book.address(valid.key()), Some(valid.clone()));
        let entry = book.address_store.get(&valid.key()).unwrap();
        assert!(entry.signature_verified);
        assert_eq!(entry.connection_failed_count, 2);
        assert_eq!(entry.consecutive_relay_failures, 2);
        drop(book);
        drop(addresses);
        assert_eq!(manager(db).lock().address(valid.key()), Some(valid));
    }
}

#[test]
fn local_nonce_is_persisted_and_unchanged_records_are_reused() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let key = SigningKey::from_bytes(&[1; 32]);
    let addresses = manager(db.clone());
    let sign = |address: &PeerAddress| -> Result<_, std::convert::Infallible> {
        Ok(key.sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
    };
    let candidate = signed("8.8.8.8:16111", 1, 99);
    let first = addresses.lock().sign_local_address(candidate.clone(), sign).unwrap();
    assert_eq!(first.nonce().unwrap(), 1);
    assert!(first.verify("kaspa-simnet"));
    let repeated = addresses
        .lock()
        .sign_local_address::<std::convert::Infallible>(candidate, |_| panic!("unchanged record must not be signed again"))
        .unwrap();
    assert_eq!(first, repeated);
    drop(addresses);
    let addresses = manager(db);
    let repeated = addresses
        .lock()
        .sign_local_address::<std::convert::Infallible>(first.clone(), |_| {
            panic!("persisted unchanged record must not be signed again")
        })
        .unwrap();
    assert_eq!(first, repeated);
    let mut changed = first;
    changed.relay_address = None;
    let next = addresses.lock().sign_local_address(changed, sign).unwrap();
    assert_eq!(next.nonce().unwrap(), 2);
    assert!(next.verify("kaspa-simnet"));
}

#[test]
fn relay_failures_do_not_modify_signed_fields_or_tcp_failures() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    let a = signed("8.8.8.8:16111", 1, 1);
    let b = signed("8.8.8.8:16111", 2, 1);
    book.add_address(a.clone());
    book.add_address(b.clone());
    for _ in 0..8 {
        book.record_relay_result(&a, false);
    }
    assert_eq!(book.address_store.get(&a.key()).unwrap().connection_failed_count, 1);
    assert_eq!(book.address_store.get(&a.key()).unwrap().consecutive_relay_failures, 9);
    assert_eq!(book.address_store.get(&b.key()).unwrap().consecutive_relay_failures, 1);
    assert_eq!(book.iterator().find(|record| record.key() == a.key()).unwrap(), a);
    book.record_relay_result(&a, true);
    assert_eq!(book.address_store.get(&a.key()).unwrap().consecutive_relay_failures, 0);
    assert!(a.verify("kaspa-simnet"));
}

#[test]
fn lazy_address_selection_uses_separate_tcp_socket_and_libp2p_id_exceptions() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    let a = signed("8.8.8.8:16111", 1, 1);
    let same_id = signed("1.1.1.1:17000", 1, 1);
    let same_tcp = signed("8.8.8.8:16111", 2, 1);
    let same_udp = signed("8.8.8.8:16112", 3, 1);
    let other = signed("9.9.9.9:16111", 4, 1);
    for address in [&a, &same_id, &same_tcp, &same_udp, &other] {
        book.add_address(address.clone());
    }
    let tcp_exceptions = HashSet::from([a.tcp_address()]);
    let libp2p_exceptions = HashSet::from([a.endpoint_id().unwrap()]);
    let selected = book.iterate_prioritized_random_addresses(tcp_exceptions.clone(), libp2p_exceptions.clone());
    assert_eq!(selected.collect::<HashSet<_>>(), HashSet::from([same_udp.clone(), other.clone()]));
    let selected = book.iterate_prioritized_random_addresses(tcp_exceptions, HashSet::new());
    assert_eq!(selected.collect::<HashSet<_>>(), HashSet::from([same_id, same_udp.clone(), other.clone()]));
    let selected = book.iterate_prioritized_random_addresses(HashSet::new(), libp2p_exceptions);
    assert_eq!(selected.collect::<HashSet<_>>(), HashSet::from([same_tcp, same_udp, other]));
}

#[test]
fn relay_selection_requires_endpoint_and_relay_port_and_is_weighted_without_replacement() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    let a = signed("8.8.8.8:16111", 1, 1);
    let b = signed("1.1.1.1:16111", 2, 1);
    let mut no_relay = signed("9.9.9.9:16111", 3, 1);
    no_relay.libp2p.as_mut().unwrap().relay_port = 0;
    resign(&mut no_relay, 3);
    for address in [&a, &b, &no_relay] {
        book.add_address(address.clone());
    }
    book.add_address(PeerAddress::from("8.8.4.4:16111".parse::<NetAddress>().unwrap()));
    book.record_relay_result(&a, true);
    for _ in 0..3 {
        book.record_relay_result(&b, false);
    }
    for _ in 0..20 {
        let candidates = book.iterate_prioritized_random_relays().collect::<Vec<_>>();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates.iter().map(|(address, _)| address.key()).collect::<HashSet<_>>().len(), 2);
        assert_eq!(candidates[0].0, a);
    }
    // Equal weights should permit either peer to be first.
    book.record_relay_result(&b, true);
    let first = (0..64).map(|_| book.iterate_prioritized_random_relays().next().unwrap().0.key()).collect::<HashSet<_>>();
    assert_eq!(first.len(), 2);
    book.ban(a.ip);
    assert_eq!(book.iterate_prioritized_random_relays().next().unwrap().0, b);
}

#[test]
fn eviction_removes_worst_connection_records_in_memory_and_on_disk() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let store = crate::DbAddressesStore::new(db.clone(), CachePolicy::Empty);
    for port in 1..=MAX_ADDRESSES + 3 {
        let tcp = NetAddress::new("8.8.8.8".parse().unwrap(), port as u16);
        store
            .set(
                &(tcp, None),
                &crate::stores::address_store::Entry { connection_failed_count: u64::from(port <= 3) * 3, ..Default::default() },
            )
            .unwrap();
    }
    let addresses = manager(db);
    assert_eq!(addresses.lock().iterator().count(), MAX_ADDRESSES);
    assert!(store.iterator().all(|row| row.unwrap().0.0.port > 3));
}

#[test]
fn own_identity_and_banned_ip_are_excluded() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    let own = signed("8.8.8.8:16111", 1, 1);
    book.set_local_libp2p_id(own.endpoint_id().unwrap());
    book.add_address(own);
    assert_eq!(book.iterator().count(), 0);
    let peer = signed("1.1.1.1:16111", 2, 1);
    book.add_address(peer.clone());
    book.ban(peer.ip);
    assert_eq!(book.iterator().count(), 0);
    assert!(book.is_banned(peer.ip));
}

#[test]
fn local_publication_tracks_only_effective_relay_changes_and_hides_saved_routes_until_reconciled() {
    use std::{cell::Cell, convert::Infallible};
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db.clone());
    let signatures = Cell::new(0);
    let sign = |address: &PeerAddress| -> Result<_, Infallible> {
        signatures.set(signatures.get() + 1);
        Ok(SigningKey::from_bytes(&[1; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
    };
    let mut initial = signed("8.8.8.8:16111", 1, 99);
    initial.relay_address = None;
    let first = addresses.lock().sign_local_address(initial, sign).unwrap();
    assert_eq!(first.nonce(), Some(1));
    for (relay, nonce) in [
        (Some(("9.9.9.9:16112", [3; 32])), 2),
        (Some(("9.9.9.9:16112", [4; 32])), 3),
        (Some(("1.1.1.1:16112", [3; 32])), 4),
        (None, 5),
    ] {
        let relay = relay.map(|(address, id)| LibP2pAddress { address: address.parse().unwrap(), id });
        addresses.lock().update_local_relay(relay, sign).unwrap();
        let published = addresses.lock().local_advertisement().unwrap();
        assert_eq!(published.nonce(), Some(nonce));
        assert_eq!(published.relay_address, relay);
        assert!(published.verify("kaspa-simnet"));
        // Read-only publications, unchanged renewals, and candidate failures do not sign or write a new record.
        for _ in 0..3 {
            assert_eq!(addresses.lock().local_advertisement(), Some(published.clone()));
            addresses.lock().update_local_relay::<Infallible>(relay, |_| panic!("unchanged relay must not sign")).unwrap();
        }
        assert_eq!(signatures.get(), nonce);
    }
    addresses.lock().update_local_relay(Some(LibP2pAddress { address: "9.9.9.9:16112".parse().unwrap(), id: [3; 32] }), sign).unwrap();
    let saved = addresses.lock().local_advertisement().unwrap();
    drop(addresses);
    let addresses = manager(db.clone());
    assert!(!addresses.lock().local_advertisement_initialized());
    assert!(addresses.lock().local_advertisement().is_none_or(|address| address.libp2p.is_none()), "saved reservation is unconfirmed");
    addresses.lock().update_local_relay::<Infallible>(None, |_| panic!("no usable local address yet")).unwrap();
    assert!(addresses.lock().local_advertisement().is_none_or(|address| address.libp2p.is_none()));
    // Confirmation before first publication reuses the saved signature and nonce.
    addresses.lock().sign_local_address::<Infallible>(saved.clone(), |_| panic!("unchanged startup candidate")).unwrap();
    assert_eq!(addresses.lock().local_advertisement(), Some(saved.clone()));
    drop(addresses);
    let addresses = manager(db);
    let mut candidate = saved.clone();
    candidate.relay_address = None;
    let reconciled = addresses.lock().sign_local_address(candidate, sign).unwrap();
    assert_eq!(reconciled.nonce(), Some(saved.nonce().unwrap() + 1));
    assert_eq!(reconciled.relay_address, None);
}

#[test]
fn failed_local_signing_preserves_published_record_and_error_source() {
    use crate::LocalAdvertisementError;
    use std::{convert::Infallible, error::Error};
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let initial = signed("8.8.8.8:16111", 1, 1);
    let first = addresses
        .lock()
        .sign_local_address(initial, |address| {
            Ok::<_, Infallible>(SigningKey::from_bytes(&[1; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
        })
        .unwrap();
    let error = addresses
        .lock()
        .update_local_relay(None, |_| Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "signer unavailable")))
        .unwrap_err();
    assert!(matches!(error, LocalAdvertisementError::Signing(_)));
    assert_eq!(error.source().unwrap().downcast_ref::<std::io::Error>().unwrap().kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(addresses.lock().local_advertisement(), Some(first));
}

#[test]
fn local_publication_rejects_missing_metadata_and_nonce_overflow() {
    use crate::LocalAdvertisementError;
    use std::convert::Infallible;
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let error = addresses
        .lock()
        .sign_local_address::<Infallible>(PeerAddress::from("8.8.8.8:16111".parse::<NetAddress>().unwrap()), |_| {
            panic!("missing metadata")
        })
        .unwrap_err();
    assert!(matches!(error, LocalAdvertisementError::MissingMetadata));
    let maximum = signed("8.8.8.8:16111", 1, u64::MAX);
    addresses.lock().address_store.set_local_advertisement(&maximum).unwrap();
    let mut changed = maximum;
    changed.relay_address = None;
    let error = addresses.lock().sign_local_address::<Infallible>(changed, |_| panic!("nonce exhausted")).unwrap_err();
    assert!(matches!(error, LocalAdvertisementError::NonceExhausted));
    assert!(!addresses.lock().local_advertisement_initialized());
}

#[test]
fn readonly_local_publication_reuses_saved_record_and_reports_failed_replacement() {
    use crate::LocalAdvertisementError;
    use std::{convert::Infallible, error::Error};
    let directory = kaspa_database::utils::get_kaspa_tempdir();
    let builder = || ConnBuilder::default().with_files_limit(10).with_db_path(directory.path().to_owned());
    let addresses = manager(builder().build().unwrap());
    let sign = |address: &PeerAddress| -> Result<_, Infallible> {
        Ok(SigningKey::from_bytes(&[1; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
    };
    let saved = addresses.lock().sign_local_address(signed("8.8.8.8:16111", 1, 99), sign).unwrap();
    drop(addresses);
    let addresses = manager(builder().build_readonly().unwrap());
    addresses.lock().sign_local_address::<Infallible>(saved.clone(), |_| panic!("unchanged startup must not sign or write")).unwrap();
    for _ in 0..3 {
        assert_eq!(addresses.lock().local_advertisement(), Some(saved.clone()));
        addresses
            .lock()
            .update_local_relay::<Infallible>(saved.relay_address, |_| panic!("unchanged relay must not sign or write"))
            .unwrap();
    }
    let error = addresses.lock().update_local_relay(None, sign).unwrap_err();
    assert!(matches!(error, LocalAdvertisementError::Store(_)));
    assert!(error.source().unwrap().is::<kaspa_database::prelude::StoreError>());
    drop(addresses);
    let addresses = manager(builder().build_readonly().unwrap());
    addresses
        .lock()
        .sign_local_address::<Infallible>(saved.clone(), |_| panic!("the persisted record must remain unchanged"))
        .unwrap();
    assert_eq!(addresses.lock().local_advertisement(), Some(saved));
}

#[test]
fn local_advertisement_falls_back_to_tcp_without_initializing_signed_publication() {
    use std::convert::Infallible;
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let addresses = manager(db);
    let mut book = addresses.lock();
    book.local_net_addresses.clear();
    assert_eq!(book.local_advertisement(), None);
    let tcp = "8.8.8.8:16111".parse::<NetAddress>().unwrap();
    book.local_net_addresses.push(tcp);
    assert_eq!(book.local_advertisement(), Some(PeerAddress::from(tcp)));
    assert!(!book.local_advertisement_initialized());
    book.update_local_relay::<Infallible>(Some(LibP2pAddress { address: "9.9.9.9:16112".parse().unwrap(), id: [3; 32] }), |_| {
        panic!("TCP fallback must not be signed as a relay update")
    })
    .unwrap();
    assert_eq!(book.local_advertisement(), Some(PeerAddress::from(tcp)));
    assert!(!book.local_advertisement_initialized());
    let signed = book
        .sign_local_address(signed("8.8.8.8:16111", 1, 1), |address| {
            Ok::<_, Infallible>(SigningKey::from_bytes(&[1; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
        })
        .unwrap();
    assert!(book.local_advertisement_initialized());
    assert_eq!(book.local_advertisement(), Some(signed));
}

#[test]
fn tcp_publication_preserves_saved_signature_and_nonce_across_restart() {
    use std::convert::Infallible;
    let directory = kaspa_database::utils::get_kaspa_tempdir();
    let builder = || ConnBuilder::default().with_files_limit(10).with_db_path(directory.path().to_owned());
    let addresses = manager(builder().build().unwrap());
    let sign = |address: &PeerAddress| -> Result<_, Infallible> {
        Ok(SigningKey::from_bytes(&[1; 32]).sign(&address.signing_bytes("kaspa-simnet")).to_bytes().to_vec())
    };
    let saved = addresses.lock().sign_local_address(signed("8.8.8.8:16111", 1, 1), sign).unwrap();
    drop(addresses);
    // The TCP fallback must work even on a read-only DB, retaining the saved signed record.
    let addresses = manager(builder().build_readonly().unwrap());
    let fallback = "1.1.1.1:17000".parse::<NetAddress>().unwrap();
    addresses.lock().local_net_addresses = vec![fallback];
    assert_eq!(addresses.lock().local_advertisement(), Some(PeerAddress::from(fallback)));
    assert!(!addresses.lock().local_advertisement_initialized());
    addresses.lock().local_net_addresses.clear();
    assert_eq!(addresses.lock().local_advertisement(), None, "without a TCP address, the saved route must remain hidden");
    addresses.lock().sign_local_address::<Infallible>(saved.clone(), |_| panic!("unchanged saved record must be reused")).unwrap();
    assert_eq!(addresses.lock().local_advertisement(), Some(saved.clone()));
    drop(addresses);
    let addresses = manager(builder().build().unwrap());
    addresses.lock().local_net_addresses = vec![fallback];
    drop(addresses);
    // A crash/restart while publishing TCP must not reset the next signed nonce.
    let addresses = manager(builder().build().unwrap());
    let mut changed = saved.clone();
    changed.relay_address = None;
    let next = addresses.lock().sign_local_address(changed, sign).unwrap();
    assert_eq!(next.nonce(), saved.nonce().map(|nonce| nonce + 1));
    assert!(next.verify("kaspa-simnet"));
}
