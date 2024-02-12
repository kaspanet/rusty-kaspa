#![allow(non_snake_case)]

use crate::error::RpcError as Error;
use crate::error::RpcResult as Result;
use crate::model::*;
use js_sys::Object;
use kaspa_addresses::Address;
use kaspa_addresses::IAddressArray;
use kaspa_consensus_wasm::SignableTransaction;
use kaspa_consensus_wasm::Transaction;
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
pub use serde_wasm_bindgen::from_value;
use wasm_bindgen::prelude::*;
use workflow_wasm::extensions::*;
use workflow_wasm::serde::to_value;

macro_rules! try_from {
    ($name:ident : $from_type:ty, $to_type:ty, $body:block) => {
        impl TryFrom<$from_type> for $to_type {
            type Error = Error;
            fn try_from($name: $from_type) -> Result<Self> {
                $body
            }
        }
    };
}

#[wasm_bindgen(typescript_custom_section)]
const TS_CATEGORY_RPC: &'static str = r#"
/**
 * @categoryDescription Node RPC
 * RPC for direct node communication.
 * @module
 */
"#;

// ---

declare! {
    IPingRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IRpcPingRequest {
        message?: string;
    }
    "#,
}

try_from! ( args: IPingRequest, PingRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IPingResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IRpcPingResponse {
        message?: string;
    }
    "#,
}

try_from! ( args: PingResponse, IPingResponse, {
    Ok(to_value(&args)?.into())
});

// --- #########################################

declare! {
    IGetBlockCountRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockCountRequest { }
    "#,
}

try_from! ( args: IGetBlockCountRequest, GetBlockCountRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBlockCountResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockCountResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBlockCountResponse, IGetBlockCountResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBlockDagInfoRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockDagInfoRequest { }
    "#,
}

try_from! ( args: IGetBlockDagInfoRequest, GetBlockDagInfoRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBlockDagInfoResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockDagInfoResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBlockDagInfoResponse, IGetBlockDagInfoResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetCoinSupplyRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetCoinSupplyRequest { }
    "#,
}

try_from! ( args: IGetCoinSupplyRequest, GetCoinSupplyRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetCoinSupplyResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetCoinSupplyResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetCoinSupplyResponse, IGetCoinSupplyResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetConnectedPeerInfoRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetConnectedPeerInfoRequest { }
    "#,
}

try_from! ( args: IGetConnectedPeerInfoRequest, GetConnectedPeerInfoRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetConnectedPeerInfoResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetConnectedPeerInfoResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetConnectedPeerInfoResponse, IGetConnectedPeerInfoResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetInfoRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetInfoRequest { }
    "#,
}

try_from! ( args: IGetInfoRequest, GetInfoRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetInfoResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetInfoResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetInfoResponse, IGetInfoResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetPeerAddressesRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetPeerAddressesRequest { }
    "#,
}

try_from! ( args: IGetPeerAddressesRequest, GetPeerAddressesRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetPeerAddressesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetPeerAddressesResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetPeerAddressesResponse, IGetPeerAddressesResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetMetricsRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMetricsRequest { }
    "#,
}

try_from! ( args: IGetMetricsRequest, GetMetricsRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetMetricsResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMetricsResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetMetricsResponse, IGetMetricsResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetSinkRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSinkRequest { }
    "#,
}

try_from! ( args: IGetSinkRequest, GetSinkRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetSinkResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSinkResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetSinkResponse, IGetSinkResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetSinkBlueScoreRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSinkBlueScoreRequest { }
    "#,
}

try_from! ( args: IGetSinkBlueScoreRequest, GetSinkBlueScoreRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetSinkBlueScoreResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSinkBlueScoreResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetSinkBlueScoreResponse, IGetSinkBlueScoreResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IShutdownRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IShutdownRequest { }
    "#,
}

try_from! ( args: IShutdownRequest, ShutdownRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IShutdownResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IShutdownResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: ShutdownResponse, IShutdownResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetServerInfoRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetServerInfoRequest { }
    "#,
}

try_from! ( args: IGetServerInfoRequest, GetServerInfoRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetServerInfoResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetServerInfoResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetServerInfoResponse, IGetServerInfoResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetSyncStatusRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSyncStatusRequest { }
    "#,
}

try_from! ( args: IGetSyncStatusRequest, GetSyncStatusRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetSyncStatusResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSyncStatusResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetSyncStatusResponse, IGetSyncStatusResponse, {
    Ok(to_value(&args)?.into())
});

// ---
// ---
// --- WITH ARGS
// ---
// ---

declare! {
    IAddPeerRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IAddPeerRequest { }
    "#,
}

try_from! ( args: IAddPeerRequest, AddPeerRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IAddPeerResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IAddPeerResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: AddPeerResponse, IAddPeerResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IBanRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IBanRequest { }
    "#,
}

try_from! ( args: IBanRequest, BanRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IBanResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IBanResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: BanResponse, IBanResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IEstimateNetworkHashesPerSecondRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IEstimateNetworkHashesPerSecondRequest { }
    "#,
}

try_from! ( args: IEstimateNetworkHashesPerSecondRequest, EstimateNetworkHashesPerSecondRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IEstimateNetworkHashesPerSecondResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IEstimateNetworkHashesPerSecondResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: EstimateNetworkHashesPerSecondResponse, IEstimateNetworkHashesPerSecondResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBalanceByAddressRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalanceByAddressRequest { }
    "#,
}

try_from! ( args: IGetBalanceByAddressRequest, GetBalanceByAddressRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBalanceByAddressResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalanceByAddressResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBalanceByAddressResponse, IGetBalanceByAddressResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBalancesByAddressesRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalancesByAddressesRequest { }
    "#,
}

try_from! ( args: IGetBalancesByAddressesRequest, GetBalancesByAddressesRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBalancesByAddressesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalancesByAddressesResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBalancesByAddressesResponse, IGetBalancesByAddressesResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBlockRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockRequest { }
    "#,
}

try_from! ( args: IGetBlockRequest, GetBlockRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBlockResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBlockResponse, IGetBlockResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBlocksRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlocksRequest { }
    "#,
}

try_from! ( args: IGetBlocksRequest, GetBlocksRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBlocksResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlocksResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBlocksResponse, IGetBlocksResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBlockTemplateRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockTemplateRequest { }
    "#,
}

try_from! ( args: IGetBlockTemplateRequest, GetBlockTemplateRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetBlockTemplateResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBlockTemplateResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetBlockTemplateResponse, IGetBlockTemplateResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetDaaScoreTimestampEstimateRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetDaaScoreTimestampEstimateRequest { }
    "#,
}

try_from! ( args: IGetDaaScoreTimestampEstimateRequest, GetDaaScoreTimestampEstimateRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetDaaScoreTimestampEstimateResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetDaaScoreTimestampEstimateResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetDaaScoreTimestampEstimateResponse, IGetDaaScoreTimestampEstimateResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetCurrentNetworkRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetCurrentNetworkRequest { }
    "#,
}

try_from! ( args: IGetCurrentNetworkRequest, GetCurrentNetworkRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetCurrentNetworkResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetCurrentNetworkResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetCurrentNetworkResponse, IGetCurrentNetworkResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetHeadersRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetHeadersRequest { }
    "#,
}

try_from! ( args: IGetHeadersRequest, GetHeadersRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetHeadersResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetHeadersResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetHeadersResponse, IGetHeadersResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetMempoolEntriesRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntriesRequest { }
    "#,
}

try_from! ( args: IGetMempoolEntriesRequest, GetMempoolEntriesRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetMempoolEntriesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntriesResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetMempoolEntriesResponse, IGetMempoolEntriesResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetMempoolEntriesByAddressesRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntriesByAddressesRequest { }
    "#,
}

try_from! ( args: IGetMempoolEntriesByAddressesRequest, GetMempoolEntriesByAddressesRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetMempoolEntriesByAddressesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntriesByAddressesResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetMempoolEntriesByAddressesResponse, IGetMempoolEntriesByAddressesResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetMempoolEntryRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntryRequest { }
    "#,
}

try_from! ( args: IGetMempoolEntryRequest, GetMempoolEntryRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetMempoolEntryResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetMempoolEntryResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetMempoolEntryResponse, IGetMempoolEntryResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetSubnetworkRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSubnetworkRequest { }
    "#,
}

try_from! ( args: IGetSubnetworkRequest, GetSubnetworkRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetSubnetworkResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetSubnetworkResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetSubnetworkResponse, IGetSubnetworkResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetUtxosByAddressesRequest,
    "IGetUtxosByAddressesRequest | Address[] | string[]",
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetUtxosByAddressesRequest { 
        addresses : Address[] | string[]
    }
    "#,
}

try_from! ( args: IGetUtxosByAddressesRequest, GetUtxosByAddressesRequest, {
    let js_value = JsValue::from(args);


    let request = if let Ok(addresses) = Vec::<Address>::try_from(IAddressArray::from(js_value.clone())) {
    // let request = if let Ok(addresses) = AddressList::try_from(&js_value) {
        GetUtxosByAddressesRequest { addresses }
    } else {
        from_value::<GetUtxosByAddressesRequest>(js_value)?
    };
    Ok(request)
});

declare! {
    IGetUtxosByAddressesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetUtxosByAddressesResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetUtxosByAddressesResponse, IGetUtxosByAddressesResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetVirtualChainFromBlockRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetVirtualChainFromBlockRequest { }
    "#,
}

try_from! ( args: IGetVirtualChainFromBlockRequest, GetVirtualChainFromBlockRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IGetVirtualChainFromBlockResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetVirtualChainFromBlockResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: GetVirtualChainFromBlockResponse, IGetVirtualChainFromBlockResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IResolveFinalityConflictRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IResolveFinalityConflictRequest { }
    "#,
}

try_from! ( args: IResolveFinalityConflictRequest, ResolveFinalityConflictRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IResolveFinalityConflictResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IResolveFinalityConflictResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: ResolveFinalityConflictResponse, IResolveFinalityConflictResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    ISubmitBlockRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface ISubmitBlockRequest { }
    "#,
}

try_from! ( args: ISubmitBlockRequest, SubmitBlockRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    ISubmitBlockResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface ISubmitBlockResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: SubmitBlockResponse, ISubmitBlockResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    ISubmitTransactionRequest,
    // "ISubmitTransactionRequest | Transaction | SignableTransaction",
    r#"
    /**
     * Submit transaction to the node.
     * 
     * @category Node RPC
     */
    export interface ISubmitTransactionRequest {
        transaction : Transaction | SignableTransaction,
        allowOrphan? : boolean
    }
    "#,
}

try_from! ( args: ISubmitTransactionRequest, SubmitTransactionRequest, {
    let js_value = JsValue::from(args);
    let object = Object::try_from(&js_value).ok_or("supplied argument must be an Object")?;
    let (transaction, allow_orphan) = if let Some(transaction) = object.try_get_value("transaction")? {
        let allow_orphan = object.try_get_bool("allowOrphan")?.unwrap_or(false);
        (transaction, allow_orphan)
    } else {
        (object.into(), false)
    };

    let request = if let Ok(signable) = SignableTransaction::try_from(&transaction) {
        SubmitTransactionRequest {
            transaction : Transaction::from(signable).into(),
            allow_orphan,
        }
    } else if let Ok(transaction) = Transaction::try_from(&transaction) {
        SubmitTransactionRequest {
            transaction : transaction.into(),
            allow_orphan,
        }
    } else {
        from_value(transaction)?
    };
    Ok(request)
});

declare! {
    ISubmitTransactionResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface ISubmitTransactionResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: SubmitTransactionResponse, ISubmitTransactionResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IUnbanRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IUnbanRequest { }
    "#,
}

try_from! ( args: IUnbanRequest, UnbanRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IUnbanResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IUnbanResponse {
        [key: string]: any
    }
    "#,
}

try_from! ( args: UnbanResponse, IUnbanResponse, {
    Ok(to_value(&args)?.into())
});

// // ---

// declare! {
//     IRequest,
//     r#"
//     export interface IRequest { }
//     "#,
// }

// try_from! ( args: IRequest, Request, {
//     Ok(Request {  })
// });

// declare! {
//     IResponse,
//     r#"
//     export interface IResponse {
//         [key: string]: any
//     }
//     "#,
// }

// try_from! ( args: Response, IResponse, {
//     let response = IResponse::default();
//     Ok(response)
// });
