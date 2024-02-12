#![allow(non_snake_case)]

use super::extensions::*;
use crate::account::descriptor::IAccountDescriptor;
use crate::api::message::*;
use crate::imports::*;
use crate::tx::{PaymentDestination, PaymentOutputs};
use crate::wasm::tx::fees::IFees;
use js_sys::Array;
// use wasm_bindgen::convert::TryFromJsValue;
// use crate::tx::{Fees, GeneratorSummary, PaymentDestination};
// use kaspa_addresses::Address;
// use wasm_bindgen::prelude::*;

use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;

pub struct PlaceholderRequest;
pub struct PlaceholderResponse;

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
const TS_CATEGORY_WALLET: &'static str = r#"
/**
 * @categoryDescription Wallet API
 * Wallet API for interfacing with Rusty Kaspa Wallet implementation.
 */
"#;

// fn try_get_secret(object: &Object, key: &str) -> Result<Secret> {
//     let value = object.get_value(key)?;
//     let secret = value.as_string().ok_or(Error::InvalidArgument(key.to_string()))?;
//     let string = object
//         .get_value(key)?
//         .as_string()
//         .ok_or(Error::InvalidArgument(key.to_string()))
//         .map(|s| s.trim().to_string())?;
//     if string.is_empty() {
//         Err(Error::SecretIsEmpty(key.to_string()))
//     } else {
//         Ok(Secret::from(string))
//     }
// }

// ---

declare! {
    IPingRequest,
    r#"
    /**
     * 
     */
    export interface IPingRequest {
        message?: string;
    }
    "#,
}

try_from! ( args: IPingRequest, PingRequest, {
    let message = args.try_get_string("message")?;
    Ok(PingRequest { message })
});

declare! {
    IPingResponse,
    r#"
    /**
     * 
     */
    export interface IPingResponse {
        message?: string;
    }
    "#,
}

try_from! ( args: PingResponse, IPingResponse, {
    let response = IPingResponse::default();
    if let Some(message) = args.message {
        response.set("message", &JsValue::from_str(&message))?;
    }
    Ok(response)
});

// ---

declare! {
    IBatchRequest,
    r#"
    /**
     * Suspend storage operations until invocation of flush().
     * 
     * @category Wallet API
     */
    export interface IBatchRequest { }
    "#,
}

try_from! ( _args: IBatchRequest, BatchRequest, {
    Ok(BatchRequest { })
});

declare! {
    IBatchResponse,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IBatchResponse { }
    "#,
}

try_from! ( _args: BatchResponse, IBatchResponse, {
    let response = IBatchResponse::default();
    Ok(response)
});

// ---

declare! {
    IFlushRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IFlushRequest {
        walletSecret : string;
    }
    "#,
}

try_from! ( args: IFlushRequest, FlushRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    Ok(FlushRequest { wallet_secret })
});

declare! {
    IFlushResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IFlushResponse { }
    "#,
}

try_from! ( _args: FlushResponse, IFlushResponse, {
    let response = IFlushResponse::default();
    Ok(response)
});

// ---

declare! {
    IConnectRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IConnectRequest {
        url : string;
        networkId : NetworkId | string;
    }
    "#,
}

try_from! ( args: IConnectRequest, ConnectRequest, {
    let url = args.get_string("url")?;
    let network_id = args.get_network_id("networkId")?;
    Ok(ConnectRequest { url, network_id })
});

declare! {
    IConnectResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IConnectResponse { }
    "#,
}

try_from! ( _args: ConnectResponse, IConnectResponse, {
    let response = IConnectResponse::default();
    Ok(response)
});

// ---

declare! {
    IDisconnectRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IDisconnectRequest { }
    "#,
}

try_from! ( _args: IDisconnectRequest, DisconnectRequest, {
    Ok(DisconnectRequest { })
});

declare! {
    IDisconnectResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IDisconnectResponse { }
    "#,
}

try_from! ( _args: DisconnectResponse, IDisconnectResponse, {
    let response = IDisconnectResponse::default();
    Ok(response)
});

// ---

declare! {
    IGetStatusRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IGetStatusRequest { }
    "#,
}

try_from! ( _args: IGetStatusRequest, GetStatusRequest, {
    Ok(GetStatusRequest { })
});

declare! {
    IGetStatusResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IGetStatusResponse {
        isConnected : boolean;
        isSynced : boolean;
        isOpen : boolean;
        url? : string;
        networkId? : NetworkId;
    }
    "#,
}

try_from! ( args: GetStatusResponse, IGetStatusResponse, {
    let GetStatusResponse { is_connected, is_synced, is_open, url, network_id, .. } = args;
    let response = IGetStatusResponse::default();
    response.set("isConnected", &is_connected.into())?;
    response.set("isSynced", &is_synced.into())?;
    response.set("isOpen", &is_open.into())?;
    if let Some(url) = url {
        response.set("url", &url.into())?;
    }
    if let Some(network_id) = network_id {
        response.set("networkId", &network_id.into())?;
    }
    Ok(response)
});

// ---

declare! {
    IWalletEnumerateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletEnumerateRequest { }
    "#,
}

try_from! ( _args: IWalletEnumerateRequest, WalletEnumerateRequest, {
    Ok(WalletEnumerateRequest { })
});

declare! {
    IWalletEnumerateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletEnumerateResponse {
        walletDescriptors: WalletDescriptor[];
    }
    "#,
}

try_from! ( args: WalletEnumerateResponse, IWalletEnumerateResponse, {
    let response = IWalletEnumerateResponse::default();
    let wallet_descriptors = Array::from_iter(args.wallet_descriptors.into_iter().map(JsValue::from));
    response.set("walletDescriptors", &JsValue::from(&wallet_descriptors))?;
    Ok(response)
});

// ---

declare! {
    IWalletCreateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletCreateRequest {
        // walletSecret: string,
        // walletFilename: string | undefined,
    }
    "#,
}

// TODO
try_from! ( _args: IWalletCreateRequest, WalletCreateRequest, {
    todo!();
    // let wallet_secret = args.try_get_secret("walletSecret")?;
    // let wallet_filename = args.try_get_string("walletFilename")?;
    // Ok(WalletCreateRequest { wallet_secret: None, wallet_filename: None })
});

declare! {
    IWalletCreateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletCreateResponse {
        // walletDescriptor: string,
    }
    "#,
}

// TODO
try_from! ( _args: WalletCreateResponse, IWalletCreateResponse, {
    todo!();
    // let response = IWalletCreateResponse::default();
    // // response.set("walletDescriptor", &JsValue::from_serde(&args.wallet_descriptor)?)?;
    // Ok(response)
});

// ---

// ---
// NOTE: `legacy_accounts` are disabled in JS API
declare! {
    IWalletOpenRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletOpenRequest {
        walletSecret: string;
        walletFilename?: string;
        accountDescriptors: boolean;
    }
    "#,
}

try_from! ( args: IWalletOpenRequest, WalletOpenRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let wallet_filename = args.try_get_string("walletFilename")?;
    let account_descriptors = args.get_value("accountDescriptors")?.as_bool().unwrap_or(false);

    Ok(WalletOpenRequest { wallet_secret, wallet_filename, account_descriptors, legacy_accounts: None })
});

declare! {
    IWalletOpenResponse,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IWalletOpenResponse {
        accountDescriptors: IAccountDescriptor[];
    }
    "#  
}

try_from!(args: WalletOpenResponse, IWalletOpenResponse, {
    let response = IWalletOpenResponse::default();
    if let Some(account_descriptors) = args.account_descriptors {
        let account_descriptors = account_descriptors.into_iter().map(IAccountDescriptor::try_from).collect::<Result<Vec<IAccountDescriptor>>>()?;
        response.set("accountDescriptors", &Array::from_iter(account_descriptors.into_iter()))?;
    }
    Ok(response)
});

// ---

declare! {
    IWalletCloseRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletCloseRequest { }
    "#,
}

try_from! ( _args: IWalletCloseRequest, WalletCloseRequest, {
    Ok(WalletCloseRequest { })
});

declare! {
    IWalletCloseResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletCloseResponse { }
    "#,
}

try_from! ( _args: WalletCloseResponse, IWalletCloseResponse, {
    let response = IWalletCloseResponse::default();
    Ok(response)
});

// ---

declare! {
    IWalletReloadRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletReloadRequest { }
    "#,
}

try_from! ( args: IWalletReloadRequest, WalletReloadRequest, {
    let reactivate = args.get_bool("reactivate")?;
    Ok(WalletReloadRequest { reactivate })
});

declare! {
    IWalletReloadResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletReloadResponse { }
    "#,
}

try_from! ( _args: WalletReloadResponse, IWalletReloadResponse, {
    let response = IWalletReloadResponse::default();
    Ok(response)
});

// ---

declare! {
    IWalletChangeSecretRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletChangeSecretRequest {
        oldWalletSecret: string;
        newWalletSecret: string;
    }
    "#,
}

try_from! ( args: IWalletChangeSecretRequest, WalletChangeSecretRequest, {
    let old_wallet_secret = args.get_secret("oldWalletSecret")?;
    let new_wallet_secret = args.get_secret("newWalletSecret")?;
    Ok(WalletChangeSecretRequest { old_wallet_secret, new_wallet_secret })
});

declare! {
    IWalletChangeSecretResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletChangeSecretResponse { }
    "#,
}

try_from! ( _args: WalletChangeSecretResponse, IWalletChangeSecretResponse, {
    let response = IWalletChangeSecretResponse::default();
    Ok(response)
});

// ---

declare! {
    IWalletExportRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletExportRequest {
        walletSecret: string;
        includeTransactions: boolean;
    }
    "#,
}

try_from! ( args: IWalletExportRequest, WalletExportRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let include_transactions = args.get_bool("includeTransactions")?;
    Ok(WalletExportRequest { wallet_secret, include_transactions })
});

declare! {
    IWalletExportResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletExportResponse {
        walletData: string;
    }
    "#,
}

// TODO
try_from! ( args: WalletExportResponse, IWalletExportResponse, {
    let response = IWalletExportResponse::default();
    response.set("walletData", &JsValue::from_str(&args.wallet_data.to_hex()))?;
    Ok(response)
});

// ---

declare! {
    IWalletImportRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletImportRequest {
        walletSecret: string;
        walletData: string;
    }
    "#,
}

try_from! ( _args: IWalletImportRequest, WalletImportRequest, {
    todo!();
    // TODO - parse hex?
    // let wallet_secret = args.get_secret("walletSecret")?;
    // let wallet_data = args.get_string("walletData")?;
    // Ok(WalletImportRequest { wallet_secret, wallet_data: wallet_data.into() })
});

declare! {
    IWalletImportResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IWalletImportResponse { }
    "#,
}

try_from! ( _args: WalletImportResponse, IWalletImportResponse, {
    let response = IWalletImportResponse::default();
    Ok(response)
});

// ---

declare! {
    IPrvKeyDataEnumerateRequest,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IPrvKeyDataEnumerateRequest { }
    "#,
}

try_from! ( _args: IPrvKeyDataEnumerateRequest, PrvKeyDataEnumerateRequest, {
    Ok(PrvKeyDataEnumerateRequest { })
});

// TODO
declare! {
    IPrvKeyDataEnumerateResponse,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IPrvKeyDataEnumerateResponse {
        // prvKeyData: PrvKeyData[],
    }
    "#,
}

// TODO
try_from! ( _args: PrvKeyDataEnumerateResponse, IPrvKeyDataEnumerateResponse, {
    todo!();
    // let response = IPrvKeyDataEnumerateResponse::default();
    // Ok(response)
});

// ---

// TODO
declare! {
    IPrvKeyDataCreateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataCreateRequest {
        walletSecret: string;
        // prvKeyDataArgs: PrvKeyDataArgs;
    }
    "#,
}

// TODO
try_from! ( _args: IPrvKeyDataCreateRequest, PrvKeyDataCreateRequest, {
    todo!();
    // let wallet_secret = args.get_secret("walletSecret")?;
    // let prv_key_data_args = args.get_value("prvKeyDataArgs")?;
    // Ok(PrvKeyDataCreateRequest { wallet_secret, prv_key_data_args })
});

// TODO
declare! {
    IPrvKeyDataCreateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataCreateResponse {
        // prvKeyDataId: string, ???
    }
    "#,
}

try_from!(_args: PrvKeyDataCreateResponse, IPrvKeyDataCreateResponse, {
    let response = IPrvKeyDataCreateResponse::default();
    // response.set("prvKeyDataId", &JsValue::from_str(&args.prv_key_data_id.to_string()))?;
    Ok(response)
});

// ---

declare! {
    IPrvKeyDataRemoveRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataRemoveRequest {
        walletSecret: string;
        prvKeyDataId: string;
    }
    "#,
}

try_from! ( args: IPrvKeyDataRemoveRequest, PrvKeyDataRemoveRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let prv_key_data_id = args.get_prv_key_data_id("prvKeyDataId")?;
    Ok(PrvKeyDataRemoveRequest { wallet_secret, prv_key_data_id })
});

declare! {
    IPrvKeyDataRemoveResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataRemoveResponse { }
    "#,
}

try_from! ( _args: PrvKeyDataRemoveResponse, IPrvKeyDataRemoveResponse, {
    let response = IPrvKeyDataRemoveResponse::default();
    Ok(response)
});

// ---

declare! {
    IPrvKeyDataGetRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataGetRequest {
        walletSecret: string;
        prvKeyDataId: string;
    }
    "#,
}

try_from! ( args: IPrvKeyDataGetRequest, PrvKeyDataGetRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let prv_key_data_id = args.get_prv_key_data_id("prvKeyDataId")?;
    Ok(PrvKeyDataGetRequest { wallet_secret, prv_key_data_id })
});

declare! {
    IPrvKeyDataGetResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataGetResponse {
        // prvKeyData: PrvKeyData,
    }
    "#,
}

// TODO
try_from! ( _args: PrvKeyDataGetResponse, IPrvKeyDataGetResponse, {
    todo!();
    // let response = IPrvKeyDataGetResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAccountsEnumerateRequest,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IAccountsEnumerateRequest { }
    "#,
}

try_from!(_args: IAccountsEnumerateRequest, AccountsEnumerateRequest, {
    Ok(AccountsEnumerateRequest { })
});

declare! {
    IAccountsEnumerateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsEnumerateResponse {
        accountDescriptors: IAccountDescriptor[];
    }
    "#,
}

// TODO
try_from! ( args: AccountsEnumerateResponse, IAccountsEnumerateResponse, {
    let response = IAccountsEnumerateResponse::default();
    let account_descriptors = args.account_descriptors.into_iter().map(IAccountDescriptor::try_from).collect::<Result<Vec<IAccountDescriptor>>>()?;
    response.set("accountDescriptors", &Array::from_iter(account_descriptors.into_iter()))?;
    Ok(response)
});

// ---

declare! {
    IAccountsRenameRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsRenameRequest {
        accountId: string;
        name?: string;
        walletSecret: string;
    }
    "#,
}

try_from! ( args: IAccountsRenameRequest, AccountsRenameRequest, {
    let account_id = args.get_account_id("accountId")?;
    let name = args.try_get_string("name")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    Ok(AccountsRenameRequest { account_id, name, wallet_secret })
});

declare! {
    IAccountsRenameResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsRenameResponse { }
    "#,
}

try_from! ( _args: AccountsRenameResponse, IAccountsRenameResponse, {
    let response = IAccountsRenameResponse::default();
    Ok(response)
});

// ---

// TODO
declare! {
    IAccountsDiscoveryRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsDiscoveryRequest {
        // TODO
    }
    "#,
}

// TODO
try_from! ( _args: IAccountsDiscoveryRequest, AccountsDiscoveryRequest, {
    todo!();
    // Ok(AccountsDiscoveryRequest { })
});

declare! {
    IAccountsDiscoveryResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsDiscoveryResponse {
        // TODO
    }
    "#,
}

// TODO
try_from! ( _args: AccountsDiscoveryResponse, IAccountsDiscoveryResponse, {
    todo!();
    // let response = IAccountsDiscoveryResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAccountsCreateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCreateRequest {
        walletSecret: string;
        // accountKind: AccountKind | string,
    }
    "#,
}

// TODO
try_from! (_args: IAccountsCreateRequest, AccountsCreateRequest, {
    todo!();
    // let wallet_secret = args.get_secret("walletSecret")?;
    // let account_kind = args.get_value("accountKind")?;
    // Ok(AccountsCreateRequest { wallet_secret, account_kind })
});

declare! {
    IAccountsCreateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCreateResponse {
        accountDescriptor : IAccountDescriptor;
    }
    "#,

}

try_from!(args: AccountsCreateResponse, IAccountsCreateResponse, {
    let response = IAccountsCreateResponse::default();
    response.set("accountDescriptor", &IAccountDescriptor::try_from(args.account_descriptor)?.into())?;
    Ok(response)
});

// ---

declare! {
    IAccountsImportRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsImportRequest {
        walletSecret: string;
        // TODO
    }
    "#,
}

try_from! ( _args: IAccountsImportRequest, AccountsImportRequest, {
    todo!();
    // Ok(AccountsImportRequest { })
});

declare! {
    IAccountsImportResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsImportResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: AccountsImportResponse, IAccountsImportResponse, {
    todo!();
    // let response = IAccountsImportResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAccountsActivateRequest,
    "IAccountsActivateRequest",
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsActivateRequest {
        accountIds?: string[],
    }
    "#,
}

try_from! (_args: IAccountsActivateRequest, AccountsActivateRequest, {
    todo!();
    // let account_ids = args.get_array("accountIds")?.iter().map(|v| v.as_string().unwrap()).collect();
    // Ok(AccountsActivateRequest { account_ids })
});

declare! {
    IAccountsActivateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsActivateResponse { }
    "#,
}

try_from! ( _args: AccountsActivateResponse, IAccountsActivateResponse, {
    let response = IAccountsActivateResponse::default();
    Ok(response)
});

// ---

declare! {
    IAccountsDeactivateRequest,
    "IAccountsDeactivateRequest",
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsDeactivateRequest {
        accountIds?: string[];
    }
    "#,
}

try_from! ( _args: IAccountsDeactivateRequest, AccountsDeactivateRequest, {
    todo!();
    // let account_ids = args.get_array("accountIds")?.iter().map(|v| v.as_string().unwrap()).collect();
    // Ok(AccountsDeactivateRequest { account_ids })
});

declare! {
    IAccountsDeactivateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsDeactivateResponse { }
    "#,
}

try_from! ( _args: AccountsDeactivateResponse, IAccountsDeactivateResponse, {
    let response = IAccountsDeactivateResponse::default();
    Ok(response)
});

// ---

declare! {
    IAccountsGetRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsGetRequest {
        accountId: string;
    }
    "#,
}

try_from! ( args: IAccountsGetRequest, AccountsGetRequest, {
    // todo!();
    // let account_ids = args.get_array("accountIds")?.iter().map(|v| v.as_string().unwrap()).collect();
    let account_id = args.get_account_id("accountId")?;
    Ok(AccountsGetRequest { account_id })
});

declare! {
    IAccountsGetResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsGetResponse {
        accountDescriptor: IAccountDescriptor;
    }
    "#,
}

// TODO
try_from! ( args: AccountsGetResponse, IAccountsGetResponse, {
    let response = IAccountsGetResponse::default();
    response.set("accountDescriptor", &IAccountDescriptor::try_from(args.account_descriptor)?.into())?;
    Ok(response)
});

// ---

declare! {
    IAccountsCreateNewAddressRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCreateNewAddressRequest {
        accountId: string;
        addressKind?: NewAddressKind | string,
    }
    "#,
}

try_from!(args: IAccountsCreateNewAddressRequest, AccountsCreateNewAddressRequest, {
    let account_id = args.get_account_id("accountId")?;
    let value = args.get_value("addressKind")?;
    let kind: NewAddressKind = if let Some(string) = value.as_string() {
        string.parse()?
    } else if let Ok(kind) = NewAddressKind::try_from_js_value(value) {
        kind
    } else {
        NewAddressKind::Receive
    };
    Ok(AccountsCreateNewAddressRequest { account_id, kind })
});

declare! {
    IAccountsCreateNewAddressResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCreateNewAddressResponse {
        address: Address;
    }
    "#,
}

try_from! ( args: AccountsCreateNewAddressResponse, IAccountsCreateNewAddressResponse, {
    let response = IAccountsCreateNewAddressResponse::default();
    response.set("address", &args.address.into())?;
    Ok(response)
});

// ---

declare! {
    IAccountsSendRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsSendRequest {
        accountId : string;
        walletSecret : string;
        paymentSecret? : string;
        priorityFeeSompi? : bigint;
        payload? : Uint8Array | string;
        // TODO destination interface
        destination? : [[Address, bigint]];
    }
    "#,
}

try_from! ( args: IAccountsSendRequest, AccountsSendRequest, {
    let account_id = args.get_account_id("accountId")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let priority_fee_sompi = args.get::<IFees>("priorityFeeSompi")?.try_into()?;
    let payload = args.try_get_value("payload")?.map(|v| v.try_as_vec_u8()).transpose()?;

    let outputs = args.get_value("destination")?;
    let destination: PaymentDestination =
        if outputs.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_from(outputs)?.into() };

    Ok(AccountsSendRequest { account_id, wallet_secret, payment_secret, priority_fee_sompi, destination, payload })
});

declare! {
    IAccountsSendResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsSendResponse {
        // TODO
    }
    "#,
}

try_from!(_args: AccountsSendResponse, IAccountsSendResponse, {
    todo!();
    // let response = IAccountsSendResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAccountsTransferRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsTransferRequest {
        // TODO
    }
    "#,
}

try_from! ( _args: IAccountsTransferRequest, AccountsTransferRequest, {
    todo!();
    // Ok(AccountsTransferRequest { })
});

declare! {
    IAccountsTransferResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsTransferResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: AccountsTransferResponse, IAccountsTransferResponse, {
    todo!();
    // let response = IAccountsTransferResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAccountsEstimateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsEstimateRequest {
        // TODO
    }
    "#,
}

try_from! ( _args: IAccountsEstimateRequest, AccountsEstimateRequest, {
    todo!();
    // Ok(AccountsEstimateRequest { })
});

declare! {
    IAccountsEstimateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsEstimateResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: AccountsEstimateResponse, IAccountsEstimateResponse, {
    todo!();
    // let response = IAccountsEstimateResponse::default();
    // Ok(response)
});

// ---

declare! {
    ITransactionsDataGetRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsDataGetRequest {
        // TODO
    }
    "#,
}

try_from! ( _args: ITransactionsDataGetRequest, TransactionsDataGetRequest, {
    todo!();
    // Ok(TransactionsDataGetRequest { })
});

declare! {
    ITransactionsDataGetResponse,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface ITransactionsDataGetResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: TransactionsDataGetResponse, ITransactionsDataGetResponse, {
    todo!();
    // let response = ITransactionsDataGetResponse::default();
    // Ok(response)
});

// ---

declare! {
    ITransactionsReplaceNoteRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceNoteRequest {
        // TODO
    }
    "#,
}

try_from! ( _args: ITransactionsReplaceNoteRequest, TransactionsReplaceNoteRequest, {
    todo!();
    // Ok(TransactionsReplaceNoteRequest { })
});

declare! {
    ITransactionsReplaceNoteResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceNoteResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: TransactionsReplaceNoteResponse, ITransactionsReplaceNoteResponse, {
    todo!();
    // let response = ITransactionsReplaceNoteResponse::default();
    // Ok(response)
});

// ---

declare! {
    ITransactionsReplaceMetadataRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceMetadataRequest {
        // TODO
    }
    "#,
}

try_from! ( _args: ITransactionsReplaceMetadataRequest, TransactionsReplaceMetadataRequest, {
    todo!();
    // Ok(TransactionsReplaceMetadataRequest { })
});

declare! {
    ITransactionsReplaceMetadataResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceMetadataResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: TransactionsReplaceMetadataResponse, ITransactionsReplaceMetadataResponse, {
    todo!();
    // let response = ITransactionsReplaceMetadataResponse::default();
    // Ok(response)
});

// ---

declare! {
    IAddressBookEnumerateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAddressBookEnumerateRequest { }
    "#,
}

try_from! ( _args: IAddressBookEnumerateRequest, AddressBookEnumerateRequest, {
    Ok(AddressBookEnumerateRequest { })
});

declare! {
    IAddressBookEnumerateResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAddressBookEnumerateResponse {
        // TODO
    }
    "#,
}

try_from! ( _args: AddressBookEnumerateResponse, IAddressBookEnumerateResponse, {
    todo!();
    // let response = IAddressBookEnumerateResponse::default();
    // Ok(response)
});

// ---
