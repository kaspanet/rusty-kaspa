cfg_if::cfg_if! {
    if #[cfg(feature = "py-sdk")] {
        use pyo3::prelude::*;

        #[pymodule]
        fn kaspa(m: &Bound<'_, PyModule>) -> PyResult<()> {
            m.add_class::<kaspa_addresses::Address>()?;

            m.add_class::<kaspa_consensus_core::tx::ScriptPublicKey>()?;

            m.add_class::<kaspa_consensus_client::Transaction>()?;
            m.add_class::<kaspa_consensus_client::TransactionInput>()?;
            m.add_class::<kaspa_consensus_client::TransactionOutpoint>()?;
            m.add_class::<kaspa_consensus_client::TransactionOutput>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntries>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntry>()?;
            m.add_class::<kaspa_consensus_client::UtxoEntryReference>()?;

            m.add_class::<kaspa_hashes::Hash>()?;

            m.add_class::<kaspa_wallet_core::tx::payment::PaymentOutput>()?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::python::tx::utils::create_transaction_py, m)?)?;
            m.add_function(wrap_pyfunction!(kaspa_wallet_core::python::signer::py_sign_transaction, m)?)?;

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
