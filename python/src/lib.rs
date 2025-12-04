cfg_if::cfg_if! {
    if #[cfg(feature = "py-sdk")] {
        use pyo3::prelude::*;

        #[pymodule]
        fn kaspa(m: &Bound<'_, PyModule>) -> PyResult<()> {
            pyo3_log::init();

            m.add_class::<kaspa_addresses::Address>()?;

            m.add_class::<kaspa_consensus_core::hashing::wasm::SighashType>()?;
            m.add_class::<kaspa_consensus_core::tx::ScriptPublicKey>()?;

            m.add_class::<kaspa_consensus_client::Transaction>()?;
            m.add_class::<kaspa_consensus_client::TransactionInput>()?;
            m.add_class::<kaspa_consensus_client::TransactionOutpoint>()?;
            m.add_class::<kaspa_consensus_client::TransactionOutput>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntries>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntry>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntryReference>()?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::address_from_script_public_key_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::pay_to_address_script_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::pay_to_script_hash_script_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::pay_to_script_hash_signature_script_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::is_script_pay_to_pubkey_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::is_script_pay_to_pubkey_ecdsa_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_consensus_client::is_script_pay_to_script_hash_py, m)?)?;

            m.add_class::<kaspa_hashes::Hash>()?;

            m.add_class::<kaspa_bip32::Language>()?;
            m.add_class::<kaspa_bip32::Mnemonic>()?;

            m.add_class::<kaspa_txscript::bindings::python::ScriptBuilder>()?;
            m.add_class::<kaspa_txscript::bindings::opcodes::Opcodes>()?;

            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::message::py_sign_message, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::message::py_verify_message, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::signer::py_sign_transaction, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::signer::create_input_signature, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::signer::sign_script_hash, m)?)?;
            m.add_class::<kaspa_wallet_core::bindings::python::tx::generator::generator::Generator>()?;
            m.add_class::<kaspa_wallet_core::bindings::python::tx::generator::pending::PendingTransaction>()?;
            m.add_class::<kaspa_wallet_core::bindings::python::tx::generator::summary::GeneratorSummary>()?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::mass::maximum_standard_transaction_mass, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::mass::calculate_unsigned_transaction_fee, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::mass::calculate_unsigned_transaction_mass, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::mass::calculate_storage_mass, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::mass::update_unsigned_transaction_mass, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::utils::create_transaction_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::utils::create_transactions_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::tx::utils::estimate_transactions_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::utils::kaspa_to_sompi, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::utils::sompi_to_kaspa, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::bindings::python::utils::sompi_to_kaspa_string_with_suffix, m)?)?;
            m.add_class::<kaspa_wallet_core::account::AccountKind>()?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::derivation::create_multisig_address_py, m)?)?;
            m.add_class::<kaspa_wallet_core::tx::payment::PaymentOutput>()?;

            m.add_class::<kaspa_wallet_keys::derivation_path::DerivationPath>()?;
            m.add_class::<kaspa_wallet_keys::keypair::Keypair>()?;
            m.add_class::<kaspa_wallet_keys::privatekey::PrivateKey>()?;
            m.add_class::<kaspa_wallet_keys::privkeygen::PrivateKeyGenerator>()?;
            m.add_class::<kaspa_wallet_keys::publickey::PublicKey>()?;
            m.add_class::<kaspa_wallet_keys::pubkeygen::PublicKeyGenerator>()?;
            m.add_class::<kaspa_wallet_keys::publickey::XOnlyPublicKey>()?;
            m.add_class::<kaspa_wallet_keys::xprv::XPrv>()?;
            m.add_class::<kaspa_wallet_keys::xpub::XPub>()?;

            m.add_class::<kaspa_wrpc_python::resolver::Resolver>()?;
            m.add_class::<kaspa_wrpc_python::client::RpcClient>()?;

            Ok(())
        }
    }
}
