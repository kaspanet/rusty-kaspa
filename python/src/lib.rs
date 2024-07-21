cfg_if::cfg_if! {
    if #[cfg(feature = "py-sdk")] {
        use pyo3::prelude::*;

        #[pymodule]
        fn kaspa(m: &Bound<'_, PyModule>) -> PyResult<()> {
            m.add_class::<kaspa_addresses::Address>()?;

            m.add_class::<kaspa_wallet_keys::privkeygen::PrivateKeyGenerator>()?;
            m.add_class::<kaspa_wallet_keys::privatekey::PrivateKey>()?;
            m.add_class::<kaspa_wallet_keys::publickey::PublicKey>()?;

            m.add_class::<kaspa_wrpc_python::client::RpcClient>()?;
            m.add_class::<kaspa_wrpc_python::resolver::Resolver>()?;

            Ok(())
        }
    }
}
