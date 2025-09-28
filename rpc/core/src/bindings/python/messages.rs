use crate::{message::*, RpcRawBlock, RpcTransaction, RpcTransactionInput, RpcTransactionOutput};
use kaspa_addresses::Address;
use kaspa_consensus_client::Transaction;
use pyo3::{
    exceptions::{PyDeprecationWarning, PyException, PyKeyError},
    ffi::c_str,
    prelude::*,
    types::{PyDict, PyList},
};
use serde_pyobject::from_pyobject;

macro_rules! try_from_no_args {
    ($to_type:ty, $body:block) => {
        impl TryFrom<Bound<'_, PyDict>> for $to_type {
            type Error = PyErr;
            fn try_from(_: Bound<'_, PyDict>) -> PyResult<Self> {
                $body
            }
        }
    };
}

macro_rules! try_from_args {
    ($name:ident : $to_type:ty, $body:block) => {
        impl TryFrom<Bound<'_, PyDict>> for $to_type {
            type Error = PyErr;
            fn try_from($name: Bound<'_, PyDict>) -> PyResult<Self> {
                $body
            }
        }
    };
}

try_from_no_args!(GetBlockCountRequest, { Ok(GetBlockCountRequest {}) });

try_from_no_args!(GetBlockDagInfoRequest, { Ok(GetBlockDagInfoRequest {}) });

try_from_no_args!(GetCoinSupplyRequest, { Ok(GetCoinSupplyRequest {}) });

try_from_no_args!(GetConnectedPeerInfoRequest, { Ok(GetConnectedPeerInfoRequest {}) });

try_from_no_args!(GetInfoRequest, { Ok(GetInfoRequest {}) });

try_from_no_args!(GetPeerAddressesRequest, { Ok(GetPeerAddressesRequest {}) });

try_from_no_args!(GetSinkRequest, { Ok(GetSinkRequest {}) });

try_from_no_args!(GetSinkBlueScoreRequest, { Ok(GetSinkBlueScoreRequest {}) });

try_from_no_args!(PingRequest, { Ok(PingRequest {}) });

try_from_no_args!(ShutdownRequest, { Ok(ShutdownRequest {}) });

try_from_no_args!(GetServerInfoRequest, { Ok(GetServerInfoRequest {}) });

try_from_no_args!(GetSyncStatusRequest, { Ok(GetSyncStatusRequest {}) });

try_from_no_args!(GetFeeEstimateRequest, { Ok(GetFeeEstimateRequest {}) });

try_from_no_args!(GetCurrentNetworkRequest, { Ok(GetCurrentNetworkRequest {}) });

try_from_no_args!(GetSystemInfoRequest, { Ok(GetSystemInfoRequest {}) });

try_from_args!(dict : AddPeerRequest, { Ok(from_pyobject(dict)?) });

try_from_args!(dict : BanRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : EstimateNetworkHashesPerSecondRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetBalanceByAddressRequest, {
    let address_value = dict.get_item("address")?
        .ok_or_else(|| PyKeyError::new_err("Key `address` not present"))?;

    let address = if let Ok(address) = address_value.extract::<Address>() {
        address
    } else if let Ok(s) = address_value.extract::<String>() {
        Address::try_from(s)
            .map_err(|err| PyException::new_err(format!("{}", err)))?
    } else {
        return Err(PyException::new_err("Addresses must be either an Address instance or a string"));
    };

    Ok(GetBalanceByAddressRequest { address })
});

try_from_args! ( dict : GetBalancesByAddressesRequest, {
    let items = dict.get_item("addresses")?
        .ok_or_else(|| PyKeyError::new_err("Key `addresses` not present"))?;

    let list = items.downcast::<PyList>()
        .map_err(|_| PyException::new_err("`addresses` should be a list"))?;

    let addresses = list.iter().map(|item| {
        if let Ok(address) = item.extract::<Address>() {
            Ok(address)
        } else if let Ok(s) = item.extract::<String>() {
            let address = Address::try_from(s)
                .map_err(|err| PyException::new_err(format!("{}", err)))?;
            Ok(address)
        } else {
            Err(PyException::new_err("Addresses must be either an Address instance or an address as a string"))
        }
    }).collect::<PyResult<Vec<Address>>>()?;

    Ok(GetBalancesByAddressesRequest { addresses })
});

try_from_args! ( dict : GetBlockRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetBlocksRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetBlockTemplateRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetConnectionsRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetCurrentBlockColorRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetDaaScoreTimestampEstimateRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetFeeEstimateExperimentalRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetHeadersRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetMempoolEntriesRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetMempoolEntriesByAddressesRequest, {
    let items = dict.get_item("addresses")?
        .ok_or_else(|| PyKeyError::new_err("Key `addresses` not present"))?;

    let list = items.downcast::<PyList>()
        .map_err(|_| PyException::new_err("`addresses` should be a list"))?;

    let addresses = list.iter().map(|item| {
        if let Ok(address) = item.extract::<Address>() {
            Ok(address)
        } else if let Ok(s) = item.extract::<String>() {
            let address = Address::try_from(s)
                .map_err(|err| PyException::new_err(format!("{}", err)))?;
            Ok(address)
        } else {
            Err(PyException::new_err("Addresses must be either an Address instance or an address as a string"))
        }
    }).collect::<PyResult<Vec<Address>>>()?;

    let include_orphan_pool = dict.get_item("includeOrphanPool")?
        .ok_or_else(|| PyKeyError::new_err("Key `include_orphan_pool` not present"))?
        .extract::<bool>()?;

    let filter_transaction_pool = dict.get_item("filterTransactionPool")?
        .ok_or_else(|| PyKeyError::new_err("Key `filter_transaction_pool` not present"))?
        .extract::<bool>()?;

    Ok(GetMempoolEntriesByAddressesRequest { addresses, include_orphan_pool, filter_transaction_pool })
});

try_from_args! ( dict : GetMempoolEntryRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetMetricsRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetSubnetworkRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetUtxosByAddressesRequest, {
    let items = dict.get_item("addresses")?
        .ok_or_else(|| PyKeyError::new_err("Key `addresses` not present"))?;
    let list = items.downcast::<PyList>()
        .map_err(|_| PyException::new_err("`addresses` should be a list"))?;

    let addresses = list.iter().map(|item| {
        if let Ok(address) = item.extract::<Address>() {
            Ok(address)
        } else if let Ok(s) = item.extract::<String>() {
            let address = Address::try_from(s)
                .map_err(|err| PyException::new_err(format!("{}", err)))?;
            Ok(address)
        } else {
            Err(PyException::new_err("Addresses must be either an Address instance or an address as a string"))
        }
    }).collect::<PyResult<Vec<Address>>>()?;

    Ok(GetUtxosByAddressesRequest { addresses })
});

try_from_args! ( dict : GetUtxoReturnAddressRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : GetVirtualChainFromBlockRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : ResolveFinalityConflictRequest, {
    Ok(from_pyobject(dict)?)
});

try_from_args! ( dict : SubmitBlockRequest, {
    let d = dict.as_any();

    let block = d.get_item("block")?;
    let header = serde_pyobject::from_pyobject(block.get_item("header")?)?;
    let transactions = serde_pyobject::from_pyobject(block.get_item("transactions")?)?;
    let allow_non_daa_blocks = d.get_item("allowNonDaaBlocks")?.extract::<bool>()?;

    let block = RpcRawBlock { header, transactions };

    Ok(SubmitBlockRequest { block, allow_non_daa_blocks })
});

try_from_args! ( dict : SubmitTransactionRequest, {
    let transaction: Transaction = dict.get_item("transaction")?
        .ok_or_else(|| PyKeyError::new_err("Key `transaction` not present"))?
        .extract()?;
    let inner = transaction.inner();

    // Deprecate allow_orphan in favor of allowOrphan for case consistency
    // Deprecation warning added September 2025, version 1.0.1.post1
    let py = dict.py();
    if dict.get_item("allow_orphan")?.is_some() {
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c_str!("`allow_orphan` will be deprecated in favor of `allowOrphan` for case consistency. Please switch."),
            0
        )?;
    }

    let allow_orphan: bool = if let Some(item) = dict.get_item("allowOrphan")? {
        item.extract()?
    } else if let Some(item) = dict.get_item("allow_orphan")? {
        item.extract()?
    } else {
        return Err(PyKeyError::new_err("Key `allowOrphan` not present"));
    };

    let inputs: Vec<RpcTransactionInput> =
        inner.inputs.clone().into_iter().map(|input| input.into()).collect::<Vec<RpcTransactionInput>>();
    let outputs: Vec<RpcTransactionOutput> =
        inner.outputs.clone().into_iter().map(|output| output.into()).collect::<Vec<RpcTransactionOutput>>();

    let rpc_transaction = RpcTransaction {
        version: inner.version,
        inputs,
        outputs,
        lock_time: inner.lock_time,
        subnetwork_id: inner.subnetwork_id.clone(),
        gas: inner.gas,
        payload: inner.payload.clone(),
        mass: inner.mass,
        verbose_data: None,
    };

    Ok(SubmitTransactionRequest { transaction: rpc_transaction, allow_orphan })
});

try_from_args! ( dict : SubmitTransactionReplacementRequest, {
    let transaction: Transaction = dict.get_item("transaction")?
        .ok_or_else(|| PyKeyError::new_err("Key `transactions` not present"))?
        .extract()?;

    Ok(SubmitTransactionReplacementRequest { transaction: transaction.into() })
});

try_from_args! ( dict : UnbanRequest, {
    Ok(from_pyobject(dict)?)
});
