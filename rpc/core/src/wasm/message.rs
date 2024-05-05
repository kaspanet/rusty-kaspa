#![allow(non_snake_case)]

use crate::error::RpcError as Error;
use crate::error::RpcResult as Result;
use crate::model::*;
use kaspa_addresses::Address;
use kaspa_addresses::AddressOrStringArrayT;
use kaspa_consensus_client::Transaction;
use kaspa_consensus_client::UtxoEntryReference;
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;
pub use serde_wasm_bindgen::from_value;
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::*;
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

// ---

#[wasm_bindgen(typescript_custom_section)]
const TS_ACCEPTED_TRANSACTION_IDS: &'static str = r#"
    /**
     * Accepted transaction IDs.
     * 
     * @category Node RPC
     */
    export interface IAcceptedTransactionIds {
        acceptingBlockHash : HexString;
        acceptedTransactionIds : HexString[];
    }
"#;

// ---

declare! {
    IPingRequest,
    r#"
    /**
     * @category Node RPC
     */
    export interface IPingRequest {
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
     * @category Node RPC
     */
    export interface IPingResponse {
        message?: string;
    }
    "#,
}

try_from! ( args: PingResponse, IPingResponse, {
    Ok(to_value(&args)?.into())
});

declare! {
    IGetBlockCountRequest,
    r#"
    /**
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
     * @category Node RPC
     */
    export interface IGetBlockCountResponse {
        headerCount : bigint;
        blockCount : bigint;
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
     * @category Node RPC
     */
    export interface IGetBlockDagInfoResponse {
        network: string;
        blockCount: bigint;
        headerCount: bigint;
        tipHashes: HexString[];
        difficulty: number;
        pastMedianTime: bigint;
        virtualParentHashes: HexString[];
        pruningPointHash: HexString;
        virtualDaaScore: bigint;
        sink: HexString;
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
     * @category Node RPC
     */
    export interface IGetCoinSupplyResponse {
        maxSompi: bigint;
        circulatingSompi: bigint;
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
     * @category Node RPC
     */
    export interface IGetInfoResponse {
        p2pId : string;
        mempoolSize : bigint;
        serverVersion : string;
        isUtxoIndexed : boolean;
        isSynced : boolean;
        /** GRPC ONLY */
        hasNotifyCommand : boolean;
        /** GRPC ONLY */
        hasMessageId : boolean;
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
     * @category Node RPC
     */
    export interface IGetSinkResponse {
        sink : HexString;
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
     * @category Node RPC
     */
    export interface IGetSinkBlueScoreResponse {
        blueScore : bigint;
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
     * @category Node RPC
     */
    export interface IShutdownResponse { }
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
     * @category Node RPC
     */
    export interface IGetServerInfoResponse {
        rpcApiVersion : number[];
        serverVersion : string;
        networkId : string;
        hasUtxoIndex : boolean;
        isSynced : boolean;
        virtualDaaScore : bigint;
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
     * @category Node RPC
     */
    export interface IGetSyncStatusResponse {
        isSynced : boolean;
    }
    "#,
}

try_from! ( args: GetSyncStatusResponse, IGetSyncStatusResponse, {
    Ok(to_value(&args)?.into())
});

/*
    Interfaces for methods with arguments
*/

declare! {
    IAddPeerRequest,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IAddPeerRequest {
        peerAddress : INetworkAddress;
        isPermanent : boolean;
    }
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
    export interface IAddPeerResponse { }
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
    export interface IBanRequest {
        /**
         * IPv4 or IPv6 address to ban.
         */
        ip : string;
    }
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
    export interface IBanResponse { }
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
     * @category Node RPC
     */
    export interface IEstimateNetworkHashesPerSecondRequest {
        windowSize : number;
        startHash? : HexString;
    }
    "#,
}

try_from! ( args: IEstimateNetworkHashesPerSecondRequest, EstimateNetworkHashesPerSecondRequest, {
    Ok(from_value(args.into())?)
});

declare! {
    IEstimateNetworkHashesPerSecondResponse,
    r#"
    /**
     * @category Node RPC
     */
    export interface IEstimateNetworkHashesPerSecondResponse {
        networkHashesPerSecond : bigint;
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
     * @category Node RPC
     */
    export interface IGetBalanceByAddressRequest {
        address : Address | string;
    }
    "#,
}

try_from! ( args: IGetBalanceByAddressRequest, GetBalanceByAddressRequest, {
    let js_value = JsValue::from(args);
    let request = if let Ok(address) = Address::try_owned_from(js_value.clone()) {
        GetBalanceByAddressRequest { address }
    } else {
        // TODO - evaluate Object property
        from_value::<GetBalanceByAddressRequest>(js_value)?
    };
    Ok(request)
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
        balance : bigint;
    }
    "#,
}

try_from! ( args: GetBalanceByAddressResponse, IGetBalanceByAddressResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IGetBalancesByAddressesRequest,
    "IGetBalancesByAddressesRequest | Address[] | string[]",
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalancesByAddressesRequest {
        addresses : Address[] | string[];
    }
    "#,
}

try_from! ( args: IGetBalancesByAddressesRequest, GetBalancesByAddressesRequest, {
    let js_value = JsValue::from(args);
    let request = if let Ok(addresses) = Vec::<Address>::try_from(AddressOrStringArrayT::from(js_value.clone())) {
        GetBalancesByAddressesRequest { addresses }
    } else {
        from_value::<GetBalancesByAddressesRequest>(js_value)?
    };
    Ok(request)
});

declare! {
    IGetBalancesByAddressesResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IBalancesByAddressesEntry {
        address : Address;
        balance : bigint;
    }
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IGetBalancesByAddressesResponse {
        entries : IBalancesByAddressesEntry[];
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
    export interface IGetBlockRequest {
        hash : HexString;
        includeTransactions : boolean;
    }
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
        block : IBlock;
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
    export interface IGetBlocksRequest {
        lowHash? : HexString;
        includeBlocks : boolean;
        includeTransactions : boolean;
    }
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
        blockHashes : HexString[];
        blocks : IBlock[];
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
    export interface IGetBlockTemplateRequest {
        payAddress : Address | string;
        /**
         * `extraData` can contain a user-supplied plain text or a byte array represented by `Uint8array`.
         */
        extraData? : string | Uint8Array;
    }
    "#,
}

try_from! ( args: IGetBlockTemplateRequest, GetBlockTemplateRequest, {
    let pay_address = args.get_cast::<Address>("payAddress")?.into_owned();
    let extra_data = if let Some(extra_data) = args.try_get_value("extraData")? {
        if let Some(text) = extra_data.as_string() {
            text.into_bytes()
        } else {
            extra_data.try_as_vec_u8()?
        }
    } else {
        Default::default()
    };
    Ok(GetBlockTemplateRequest {
        pay_address,
        extra_data,
    })
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
        block : IBlock;
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
    export interface IGetDaaScoreTimestampEstimateRequest {
        daaScores : bigint[];
    }
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
        timestamps : bigint[];
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
        network : string;
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
    export interface IGetHeadersRequest {
        startHash : HexString;
        limit : bigint;
        isAscending : boolean;
    }
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
        headers : IHeader[];
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
    export interface IGetMempoolEntriesRequest {
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
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
        mempoolEntries : IMempoolEntry[];
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
    export interface IGetMempoolEntriesByAddressesRequest {
        addresses : Address[] | string[];
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
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
        entries : IMempoolEntry[];
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
    export interface IGetMempoolEntryRequest {
        transactionId : HexString;
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
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
        mempoolEntry : IMempoolEntry;
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
    export interface IGetSubnetworkRequest {
        subnetworkId : HexString;
    }
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
        gasLimit : bigint;
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
    let request = if let Ok(addresses) = Vec::<Address>::try_from(AddressOrStringArrayT::from(js_value.clone())) {
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
        entries : IUtxoEntry[];
    }
    "#,
}

try_from! ( args: GetUtxosByAddressesResponse, IGetUtxosByAddressesResponse, {
    let GetUtxosByAddressesResponse { entries } = args;
    let entries = entries.into_iter().map(UtxoEntryReference::from).collect::<Vec<UtxoEntryReference>>();
    let entries = js_sys::Array::from_iter(entries.into_iter().map(JsValue::from));
    let response = IGetUtxosByAddressesResponse::default();
    response.set("entries", entries.as_ref())?;
    Ok(response)
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
    export interface IGetVirtualChainFromBlockRequest {
        startHash : HexString;
        includeAcceptedTransactionIds: boolean;
    }
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
        removedChainBlockHashes : HexString[];
        addedChainBlockHashes : HexString[];
        acceptedTransactionIds : IAcceptedTransactionIds[];
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
    export interface IResolveFinalityConflictRequest {
        finalityBlockHash: HexString;
    }
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
    export interface IResolveFinalityConflictResponse { }
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
    export interface ISubmitBlockRequest {
        block : IBlock;
        allowNonDAABlocks: boolean;
    }
    "#,
}

try_from! ( args: ISubmitBlockRequest, SubmitBlockRequest, {
    Ok(from_value(args.into())?)
});

#[wasm_bindgen(typescript_custom_section)]
const TS_SUBMIT_BLOCK_REPORT: &'static str = r#"
    /**
     * 
     * @category Node RPC
     */
    export enum SubmitBlockRejectReason {
        /**
         * The block is invalid.
         */
        BlockInvalid = "BlockInvalid",
        /**
         * The node is not synced.
         */
        IsInIBD = "IsInIBD",
        /**
         * Route is full.
         */
        RouteIsFull = "RouteIsFull",
    }

    /**
     * 
     * @category Node RPC
     */
    export interface ISubmitBlockReport {
        type : "success" | "reject";
        reason? : SubmitBlockRejectReason;
    }
"#;

declare! {
    ISubmitBlockResponse,
    r#"
    /**
     * 
     * 
     * @category Node RPC
     */
    export interface ISubmitBlockResponse {
        report : ISubmitBlockReport;
    }
    "#,
}

try_from! ( args: SubmitBlockResponse, ISubmitBlockResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    ISubmitTransactionRequest,
    // "ISubmitTransactionRequest | Transaction",
    r#"
    /**
     * Submit transaction to the node.
     * 
     * @category Node RPC
     */
    export interface ISubmitTransactionRequest {
        transaction : Transaction,
        allowOrphan? : boolean
    }
    "#,
}

try_from! ( args: ISubmitTransactionRequest, SubmitTransactionRequest, {
    let (transaction, allow_orphan) = if let Some(transaction) = args.try_get_value("transaction")? {
        let allow_orphan = args.try_get_bool("allowOrphan")?.unwrap_or(false);
        (transaction, allow_orphan)
    } else {
        (args.into(), false)
    };

    let request = if let Ok(transaction) = Transaction::try_owned_from(&transaction) {
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
        transactionId : HexString;
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
    export interface IUnbanRequest {
        /**
         * IPv4 or IPv6 address to unban.
         */
        ip : string;
    }
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
    export interface IUnbanResponse { }
    "#,
}

try_from! ( args: UnbanResponse, IUnbanResponse, {
    Ok(to_value(&args)?.into())
});
