use kaspa_consensus_core::network::NetworkId;
use kaspa_python_macros::py_async;
use kaspa_wrpc_client::{Resolver as NativeResolver, WrpcEncoding};
use pyo3::prelude::*;
use std::{str::FromStr, sync::Arc};

#[derive(Debug, Clone)]
#[pyclass]
pub struct Resolver {
    resolver: NativeResolver,
}

impl Resolver {
    pub fn new(resolver: NativeResolver) -> Self {
        Self { resolver }
    }
}

#[pymethods]
impl Resolver {
    #[new]
    #[pyo3(signature = (urls=None, tls=None))]
    pub fn ctor(urls: Option<Vec<String>>, tls: Option<bool>) -> PyResult<Resolver> {
        let tls = tls.unwrap_or(false);
        if let Some(urls) = urls {
            Ok(Self { resolver: NativeResolver::new(Some(urls.into_iter().map(|url| Arc::new(url)).collect::<Vec<_>>()), tls) })
        } else {
            Ok(Self { resolver: NativeResolver::default() })
        }
    }
}

#[pymethods]
impl Resolver {
    fn urls(&self) -> Vec<String> {
        self.resolver.urls().unwrap_or_default().into_iter().map(|url| (*url).clone()).collect::<Vec<_>>()
    }

    fn get_node(&self, py: Python, encoding: String, network_id: &str) -> PyResult<Py<PyAny>> {
        let encoding = WrpcEncoding::from_str(encoding.as_str()).unwrap();
        let network_id = NetworkId::from_str(network_id)?;

        let resolver = self.resolver.clone();
        py_async! {py, async move {
            let node = resolver.get_node(encoding, network_id).await?;
            Python::with_gil(|py| {
                Ok(serde_pyobject::to_pyobject(py, &node).unwrap().to_object(py))
            })
        }}
    }

    fn get_url(&self, py: Python, encoding: String, network_id: &str) -> PyResult<Py<PyAny>> {
        let encoding = WrpcEncoding::from_str(encoding.as_str()).unwrap();
        let network_id = NetworkId::from_str(network_id)?;

        let resolver = self.resolver.clone();
        py_async! {py, async move {
            let url = resolver.get_url(encoding, network_id).await?;
            Ok(url)
        }}
    }

    // fn connect() TODO
}

impl From<Resolver> for NativeResolver {
    fn from(resolver: Resolver) -> Self {
        resolver.resolver
    }
}
