#![allow(non_snake_case)]

use super::extensions::*;
use crate::account::descriptor::IAccountDescriptor;
use crate::api::message::*;
use crate::imports::*;
use crate::tx::{PaymentDestination, PaymentOutputs};
use crate::wasm::tx::fees::IFees;
use js_sys::Array;
use serde_wasm_bindgen::from_value;
use workflow_wasm::serde::to_value;

use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;

// pub struct PlaceholderRequest;
// pub struct PlaceholderResponse;

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

// ---

// declare! {
//     IPingRequest,
//     r#"
//     /**
//      *
//      */
//     export interface IPingRequest {
//         message?: string;
//     }
//     "#,
// }

// try_from! ( args: IPingRequest, PingRequest, {
//     let message = args.try_get_string("message")?;
//     Ok(PingRequest { message })
// });

// declare! {
//     IPingResponse,
//     r#"
//     /**
//      *
//      */
//     export interface IPingResponse {
//         message?: string;
//     }
//     "#,
// }

// try_from! ( args: PingResponse, IPingResponse, {
//     let response = IPingResponse::default();
//     if let Some(message) = args.message {
//         response.set("message", &JsValue::from_str(&message))?;
//     }
//     Ok(response)
// });

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
    Ok(IBatchResponse::default())
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
    Ok(IFlushResponse::default())
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
    Ok(IConnectResponse::default())
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
    Ok(IDisconnectResponse::default())
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
     * If filename is not supplied, the filename will be derived from the wallet title.
     * If both wallet title and filename are not supplied, the wallet will be create
     * with the default filename `kaspa`.
     * 
     * @category Wallet API
     */
    export interface IWalletCreateRequest {
        /** Wallet encryption secret */
        walletSecret: string;
        /** Optional wallet title */
        title?: string;
        /** Optional wallet filename */
        filename?: string;
        /** Optional user hint */
        userHint?: string;
        /** 
         * Overwrite wallet data if the wallet with the same filename already exists.
         * (Use with caution!)
         */
        overwriteWalletStorage?: boolean;
    }
    "#,
}

// TODO
try_from! ( args: IWalletCreateRequest, WalletCreateRequest, {

    let wallet_secret = args.get_secret("walletSecret")?;
    let title = args.try_get_string("title")?;
    let filename = args.try_get_string("filename")?;
    let user_hint = args.try_get_string("userHint")?.map(Hint::from);
    let encryption_kind = EncryptionKind::default();
    let overwrite_wallet_storage = args.try_get_bool("overwriteWalletStorage")?.unwrap_or(false);

    let wallet_args = WalletCreateArgs {
        title,
        filename,
        user_hint,
        encryption_kind,
        overwrite_wallet_storage,
    };

    Ok(WalletCreateRequest { wallet_secret, wallet_args })
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
        walletDescriptor: WalletDescriptor;
        storageDescriptor: string;
    }
    "#,
}

// TODO
try_from! ( args: WalletCreateResponse, IWalletCreateResponse, {
    Ok(to_value(&args)?.into())
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
    Ok(IWalletCloseResponse::default())
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
    Ok(IWalletReloadResponse::default())
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
    Ok(IWalletChangeSecretResponse::default())
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
        walletData: HexString;
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
        walletData: HexString | Uint8Array;
    }
    "#,
}

try_from! ( args: IWalletImportRequest, WalletImportRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let wallet_data = args.get_vec_u8("walletData").map_err(|err|Error::custom(format!("walletData: {err}")))?;
    Ok(WalletImportRequest { wallet_secret, wallet_data })
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
    Ok(IWalletImportResponse::default())
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

declare! {
    IPrvKeyDataEnumerateResponse,
    r#"
    /**
     * 
     * Response returning a list of private key ids, their optional names and properties.
     * 
     * @see {@link IPrvKeyDataInfo}
     * @category Wallet API
     */
    export interface IPrvKeyDataEnumerateResponse {
        prvKeyDataList: IPrvKeyDataInfo[],
    }
    "#,
}

try_from! ( args: PrvKeyDataEnumerateResponse, IPrvKeyDataEnumerateResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IPrvKeyDataCreateRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IPrvKeyDataCreateRequest {
        /** Wallet encryption secret */
        walletSecret: string;
        /** Optional name of the private key */
        name? : string;
        /** 
         * Optional key secret (BIP39 passphrase).
         * 
         * If supplied, all operations requiring access 
         * to the key will require the `paymentSecret` 
         * to be provided.
         */
        paymentSecret? : string;
        /** BIP39 mnemonic phrase (12 or 24 words)*/
        mnemonic : string;
    }
    "#,
}

// TODO
try_from! ( args: IPrvKeyDataCreateRequest, PrvKeyDataCreateRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let name = args.try_get_string("name")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let mnemonic = args.get_string("mnemonic")?;

    let prv_key_data_args = PrvKeyDataCreateArgs {
        name,
        payment_secret,
        mnemonic,
    };

    Ok(PrvKeyDataCreateRequest { wallet_secret, prv_key_data_args })
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
        prvKeyDataId: HexString;
    }
    "#,
}

try_from!(args: PrvKeyDataCreateResponse, IPrvKeyDataCreateResponse, {
    Ok(to_value(&args)?.into())
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

// TODO
try_from! ( _args: PrvKeyDataRemoveResponse, IPrvKeyDataRemoveResponse, {
    Ok(IPrvKeyDataRemoveResponse::default())
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
    Ok(IAccountsRenameResponse::default())
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
        discoveryKind: AccountsDiscoveryKind,
        accountScanExtent: number,
        addressScanExtent: number,
        bip39_passphrase?: string,
        bip39_mnemonic: string,
    }
    "#,
}

// TODO
try_from! (args: IAccountsDiscoveryRequest, AccountsDiscoveryRequest, {

    let discovery_kind = args.get_value("discoveryKind")?;
    let discovery_kind = if let Some(discovery_kind) = discovery_kind.as_string() {
        discovery_kind.parse()?
    } else {
        AccountsDiscoveryKind::try_from_js_value(discovery_kind)?
    };
    let account_scan_extent = args.get_u32("accountScanExtent")?;
    let address_scan_extent = args.get_u32("addressScanExtent")?;
    let bip39_passphrase = args.try_get_secret("bip39_passphrase")?;
    let bip39_mnemonic = args.get_secret("bip39_mnemonic")?;

    Ok(AccountsDiscoveryRequest {
        discovery_kind,
        account_scan_extent,
        address_scan_extent,
        bip39_passphrase,
        bip39_mnemonic,
    })
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
        lastAccountIndexFound : number;
    }
    "#,
}

try_from! ( args: AccountsDiscoveryResponse, IAccountsDiscoveryResponse, {
    Ok(to_value(&args)?.into())
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
/*

pub enum AccountCreateArgs {
    Bip32 {
        prv_key_data_args: PrvKeyDataArgs,
        account_args: AccountCreateArgsBip32,
    },
    Legacy {
        prv_key_data_id: PrvKeyDataId,
        account_name: Option<String>,
    },
    Multisig {
        prv_key_data_args: Vec<PrvKeyDataArgs>,
        additional_xpub_keys: Vec<String>,
        name: Option<String>,
        minimum_signatures: u16,
    },
}

*/
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
        accountIds?: HexString[],
    }
    "#,
}

try_from! (args: IAccountsActivateRequest, AccountsActivateRequest, {
    Ok(from_value::<AccountsActivateRequest>(args.into())?)
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
    Ok(IAccountsActivateResponse::default())
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

try_from! ( args: IAccountsDeactivateRequest, AccountsDeactivateRequest, {
    Ok(from_value::<AccountsDeactivateRequest>(args.into())?)
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
    Ok(IAccountsDeactivateResponse::default())
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
    Ok(from_value::<AccountsGetRequest>(args.into())?)
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
    Ok(to_value(&args)?.into())
    // let response = IAccountsGetResponse::default();
    // response.set("accountDescriptor", &IAccountDescriptor::try_from(args.account_descriptor)?.into())?;
    // Ok(response)
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
    Ok(to_value(&args)?.into())
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
        generatorSummary : GeneratorSummary;
        transactionIds : HexString[];
    }
    "#,
}

try_from!(args: AccountsSendResponse, IAccountsSendResponse, {
    use crate::wasm::tx::GeneratorSummary;

    let response = IAccountsSendResponse::default();
    response.set("generatorSummary", &GeneratorSummary::from(args.generator_summary).into())?;
    response.set("transactionIds", &to_value(&args.transaction_ids)?)?;
    Ok(response)
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
        /**
         * The id of account the transaction belongs to.
         */
        accountId: HexString,
        /**
         * The network id of the transaction.
         */
        networkId: NetworkId | string,
        /**
         * The id of the transaction.
         */
        transactionId: HexString,
        /**
         * Optional note string to replace the existing note.
         * If not supplied, the note will be removed.
         */
        note?: string,
    }
    "#,
}

try_from! ( args: ITransactionsReplaceNoteRequest, TransactionsReplaceNoteRequest, {

    let account_id = args.get_account_id("accountId")?;
    let network_id = args.get_network_id("networkId")?;
    let transaction_id = args.get_transaction_id("transactionId")?;
    let note = args.try_get_string("note")?;

    Ok(TransactionsReplaceNoteRequest {
        account_id,
        network_id,
        transaction_id,
        note,
    })
});

declare! {
    ITransactionsReplaceNoteResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceNoteResponse { }
    "#,
}

try_from! ( _args: TransactionsReplaceNoteResponse, ITransactionsReplaceNoteResponse, {
    Ok(ITransactionsReplaceNoteResponse::default())
});

// ---

// TODO
declare! {
    ITransactionsReplaceMetadataRequest,
    r#"
    /**
     * Metadata is a wallet-specific string that can be used to store arbitrary data.
     * It should contain a serialized JSON string with `key` containing the custom
     * data stored by the wallet.  When interacting with metadata, the wallet should
     * always deserialize the JSON string and then serialize it again after making
     * changes, preserving any foreign keys that it might encounter.
     *  
     * To preserve foreign metadata, the pattern of access should be:
     * `Get -> Modify -> Replace`
     * 
     * @category Wallet API
     */
    export interface ITransactionsReplaceMetadataRequest {
        /**
         * The id of account the transaction belongs to.
         */
        accountId: HexString,
        /**
         * The network id of the transaction.
         */
        networkId: NetworkId | string,
        /**
         * The id of the transaction.
         */
        transactionId: HexString,
        /**
         * Optional metadata string to replace the existing metadata.
         * If not supplied, the metadata will be removed.
         */
        metadata?: string,    
    }
    "#,
}

try_from! ( args: ITransactionsReplaceMetadataRequest, TransactionsReplaceMetadataRequest, {
    let account_id = args.get_account_id("accountId")?;
    let network_id = args.get_network_id("networkId")?;
    let transaction_id = args.get_transaction_id("transactionId")?;
    let metadata = args.try_get_string("metadata")?;

    Ok(TransactionsReplaceMetadataRequest {
        account_id,
        network_id,
        transaction_id,
        metadata,
    })
});

declare! {
    ITransactionsReplaceMetadataResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface ITransactionsReplaceMetadataResponse { }
    "#,
}

try_from! ( _args: TransactionsReplaceMetadataResponse, ITransactionsReplaceMetadataResponse, {
    Ok(ITransactionsReplaceMetadataResponse::default())
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
    Err(Error::NotImplemented)
});

// ---
