#![allow(non_snake_case)]

use super::extensions::*;
use crate::account::descriptor::IAccountDescriptor;
use crate::api::message::*;
use crate::imports::*;
use crate::tx::{Fees, PaymentDestination, PaymentOutputs};
use crate::wasm::api::keydata::PrvKeyDataVariantKind;
use crate::wasm::tx::fees::IFees;
use crate::wasm::tx::GeneratorSummary;
use js_sys::Array;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use serde_wasm_bindgen::from_value;
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
        // destination wRPC node URL (if omitted, the resolver is used)
        url? : string;
        // network identifier
        networkId : NetworkId | string;
        // retry on error
        retryOnError? : boolean;
        // block async connect (method will not return until the connection is established)
        block? : boolean;
        // require node to be synced (fail otherwise)
        requireSync? : boolean;
    }
    "#,
}

try_from! ( args: IConnectRequest, ConnectRequest, {
    let url = args.try_get_string("url")?;
    let network_id = args.get_network_id("networkId")?;
    let retry_on_error = args.try_get_bool("retryOnError")?.unwrap_or(true);
    let block_async_connect = args.try_get_bool("block")?.unwrap_or(false);
    let require_sync = args.try_get_bool("requireSync")?.unwrap_or(true);
    Ok(ConnectRequest { url, network_id, retry_on_error, block_async_connect, require_sync })
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
    export interface IGetStatusRequest {
        /**
         * Optional context creation name.
         * @see {@link IRetainContextRequest}
         */
        name? : string;
    }
    "#,
}

try_from! ( args: IGetStatusRequest, GetStatusRequest, {
    let name = args.try_get_string("name")?;
    Ok(GetStatusRequest { name })
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
        context? : HexString;
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
    IRetainContextRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IRetainContextRequest {
        /**
         * Optional context creation name.
         */
        name : string;
        /**
         * Optional context data to retain.
         */
        data? : string;
    }
    "#,
}

try_from! ( args: IRetainContextRequest, RetainContextRequest, {
    let name = args.get_string("name")?;
    let data = args.try_get_string("data")?;
    let data = data.map(|data|Vec::<u8>::from_hex(data.as_str())).transpose()?;
    Ok(RetainContextRequest { name, data })
});

declare! {
    IRetainContextResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IRetainContextResponse {
    }
    "#,
}

try_from! ( _args: RetainContextResponse, IRetainContextResponse, {
    Ok(IRetainContextResponse::default())
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
        walletDescriptor: IWalletDescriptor;
        storageDescriptor: IStorageDescriptor;
    }
    "#,
}

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
     * @category Wallet API
     */
    export interface IWalletOpenRequest {
        walletSecret: string;
        filename?: string;
        accountDescriptors: boolean;
    }
    "#,
}

try_from! ( args: IWalletOpenRequest, WalletOpenRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let filename = args.try_get_string("filename")?;
    let account_descriptors = args.get_value("accountDescriptors")?.as_bool().unwrap_or(false);

    Ok(WalletOpenRequest { wallet_secret, filename, account_descriptors, legacy_accounts: None })
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
    export interface IWalletReloadRequest {
        /**
         * Reactivate accounts that are active before the reload.
         */
        reactivate: boolean;
    }
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
        /** BIP39 mnemonic phrase (12 or 24 words) if kind is mnemonic */
        mnemonic? : string;
        /** Secret key if kind is secretKey */
        secretKey? : string;
        /** Kind of the private key data */
        kind : "mnemonic" | "secretKey";
    }
    "#,
}

try_from! ( args: IPrvKeyDataCreateRequest, PrvKeyDataCreateRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let name = args.try_get_string("name")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let kind = args.get_string("kind")?;
    let (secret, kind) = match kind.as_str() {
        "mnemonic" => (args.get_secret("mnemonic")?, PrvKeyDataVariantKind::Mnemonic),
        "secretKey" => {
            let mut hex_key = args.get_string("secretKey")?;
            let mut secret = [0u8; 32];
            faster_hex::hex_decode(hex_key.as_bytes(), &mut secret).map_err(|err|Error::custom(format!("secretKey: {err}")))?;
            hex_key.zeroize();
            let secret = Secret::new(secret.to_vec());
            (secret, PrvKeyDataVariantKind::SecretKey)
        },
        _ => return Err(Error::custom("Invalid kind, supported: mnemonic, secretKey".to_string())),
    };

    //log_info!("secret: {:?}", secret);

    let prv_key_data_args = PrvKeyDataCreateArgs {
        name,
        payment_secret,
        kind,
        secret,
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
        prvKeyDataId: HexString;
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
        prvKeyDataId: HexString;
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
        AccountsDiscoveryKind::try_enum_from(&discovery_kind)?
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
    export type IAccountsCreateRequest = {
        walletSecret: string;
        type: "bip32";
        accountName:string;
        accountIndex?:number;
        prvKeyDataId:string;
        paymentSecret?:string;
    } | {
        walletSecret: string;
        type: "kaspa-keypair-standard";
        accountName:string;
        prvKeyDataId:string;
        paymentSecret?:string;
        ecdsa?:boolean;
    };

    //   |{
    //     walletSecret: string;
    //     type: "bip32-readonly";
    //     accountName:string;
    //     accountIndex?:number;
    //     pubkey:HexString;
    //     paymentSecret?:string;
    //  }
    "#,
}

try_from! (args: IAccountsCreateRequest, AccountsCreateRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;

    let kind = AccountKind::try_from(args.try_get_value("type")?.ok_or(Error::custom("type is required"))?)?;

    let account_create_args = match kind.as_str() {
        crate::account::BIP32_ACCOUNT_KIND => {
            let prv_key_data_args = PrvKeyDataArgs {
                prv_key_data_id: args.try_get_prv_key_data_id("prvKeyDataId")?.ok_or(Error::custom("prvKeyDataId is required"))?,
                payment_secret: args.try_get_secret("paymentSecret")?,
            };

            let account_args = AccountCreateArgsBip32 {
                account_name: args.try_get_string("accountName")?,
                account_index: args.get_u64("accountIndex").ok(),
            };

            AccountCreateArgs::Bip32 { prv_key_data_args, account_args }

        }
        crate::account::KEYPAIR_ACCOUNT_KIND => {
            AccountCreateArgs::Keypair {
                prv_key_data_id: args.try_get_prv_key_data_id("prvKeyDataId")?.ok_or(Error::custom("prvKeyDataId is required"))?,
                account_name: args.try_get_string("accountName")?,
                ecdsa: args.get_bool("ecdsa").unwrap_or(false),
            }
        }
        _ => {
            return Err(Error::custom("only BIP32/kaspa-keypair-standard accounts are currently supported"));
        }
    };

    Ok(AccountsCreateRequest { wallet_secret, account_create_args })
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
    IAccountsEnsureDefaultRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsEnsureDefaultRequest {
        walletSecret: string;
        paymentSecret?: string;
        type : AccountKind | string;
        mnemonic? : string;
    }
    "#,
}

try_from! (args: IAccountsEnsureDefaultRequest, AccountsEnsureDefaultRequest, {
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let account_kind = AccountKind::try_from(args.get_value("type")?)?;
    let mnemonic_phrase = args.try_get_secret("mnemonic")?;

    Ok(AccountsEnsureDefaultRequest { wallet_secret, payment_secret, account_kind, mnemonic_phrase })
});

declare! {
    IAccountsEnsureDefaultResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsEnsureDefaultResponse {
        accountDescriptor : IAccountDescriptor;
    }
    "#,
}

try_from!(args: AccountsEnsureDefaultResponse, IAccountsEnsureDefaultResponse, {
    let response = IAccountsEnsureDefaultResponse::default();
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
    unimplemented!();
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
    unimplemented!();
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

try_from! ( args: AccountsGetResponse, IAccountsGetResponse, {
    Ok(to_value(&args)?.into())
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
    } else if let Ok(kind) = NewAddressKind::try_enum_from(&value) {
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
        /**
         * Hex identifier of the account.
         */
        accountId : HexString;
        /**
         * Wallet encryption secret.
         */
        walletSecret : string;
        /**
         * Optional key encryption secret or BIP39 passphrase.
         */
        paymentSecret? : string;
        /**
         * Fee rate in sompi per 1 gram of mass.
         */
        feeRate? : number;
        /**
         * Priority fee.
         */
        priorityFeeSompi? : IFees | bigint;
        /**
         * 
         */
        payload? : Uint8Array | HexString;
        /**
         * If not supplied, the destination will be the change address resulting in a UTXO compound transaction.
         */
        destination? : IPaymentOutput[];
    }
    "#,
}

try_from! ( args: IAccountsSendRequest, AccountsSendRequest, {
    let account_id = args.get_account_id("accountId")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let fee_rate = args.get_f64("feeRate").ok();
    let priority_fee_sompi = args.get::<IFees>("priorityFeeSompi")?.try_into()?;
    let payload = args.try_get_value("payload")?.map(|v| v.try_as_vec_u8()).transpose()?;

    let outputs = args.get_value("destination")?;
    let destination: PaymentDestination =
        if outputs.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_owned_from(outputs)?.into() };

    Ok(AccountsSendRequest { account_id, wallet_secret, payment_secret, fee_rate, priority_fee_sompi, destination, payload })
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
        /**
         * Summary produced by the transaction generator.
         */
        generatorSummary : GeneratorSummary;
        /**
         * Hex identifiers of successfully submitted transactions.
         */
        transactionIds : HexString[];
    }
    "#,
}

try_from!(args: AccountsSendResponse, IAccountsSendResponse, {

    let response = IAccountsSendResponse::default();
    response.set("generatorSummary", &GeneratorSummary::from(args.generator_summary).into())?;
    response.set("transactionIds", &to_value(&args.transaction_ids)?)?;
    Ok(response)
});

// ---

declare! {
    IAccountsPskbSignRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbSignRequest {
        /**
         * Hex identifier of the account.
         */
        accountId : HexString;
        /**
         * Wallet encryption secret.
         */
        walletSecret : string;
        /**
         * Optional key encryption secret or BIP39 passphrase.
         */
        paymentSecret? : string;

        /**
         * PSKB to sign.
         */
        pskb : string;

        /**
         * Address to sign for.
         */
        signForAddress? : Address | string;
    }
    "#,
}

try_from! ( args: IAccountsPskbSignRequest, AccountsPskbSignRequest, {
    let account_id = args.get_account_id("accountId")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let pskb = args.get_string("pskb")?;
    let sign_for_address = match args.try_get_value("signForAddress")? {
        Some(v) => Some(Address::try_cast_from(&v)?.into_owned()),
        None => None,
    };
    Ok(AccountsPskbSignRequest { account_id, wallet_secret, payment_secret, pskb, sign_for_address })
});

declare! {
    IAccountsPskbSignResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbSignResponse {
        /**
         * signed PSKB.
         */
        pskb: string;
    }
    "#,
}

try_from!(args: AccountsPskbSignResponse, IAccountsPskbSignResponse, {

    let response = IAccountsPskbSignResponse::default();
    response.set("pskb", &args.pskb.into())?;
    Ok(response)
});

// ---

declare! {
    IAccountsPskbBroadcastRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbBroadcastRequest {
        accountId : HexString;
        pskb : string;
    }
    "#,
}

try_from! ( args: IAccountsPskbBroadcastRequest, AccountsPskbBroadcastRequest, {
    let account_id = args.get_account_id("accountId")?;
    let pskb = args.get_string("pskb")?;
    Ok(AccountsPskbBroadcastRequest { account_id, pskb })
});

declare! {
    IAccountsPskbBroadcastResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbBroadcastResponse {
        transactionIds : HexString[];
    }
    "#,
}

try_from! ( args: AccountsPskbBroadcastResponse, IAccountsPskbBroadcastResponse, {
    Ok(to_value(&args)?.into())
});

declare! {
    IAccountsPskbSendRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbSendRequest {
        /**
         * Hex identifier of the account.
         */
        accountId : HexString;
        /**
         * Wallet encryption secret.
         */
        walletSecret : string;
        /**
         * Optional key encryption secret or BIP39 passphrase.
         */
        paymentSecret? : string;

        /**
         * PSKB to sign.
         */
        pskb : string;

        /**
         * Address to sign for.
         */
        signForAddress? : Address | string;
    }
    "#,
}

try_from! ( args: IAccountsPskbSendRequest, AccountsPskbSendRequest, {
    let account_id = args.get_account_id("accountId")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let pskb = args.get_string("pskb")?;
    let sign_for_address = match args.try_get_value("signForAddress")? {
        Some(v) => Some(Address::try_cast_from(&v)?.into_owned()),
        None => None,
    };
    Ok(AccountsPskbSendRequest { account_id, wallet_secret, payment_secret, pskb, sign_for_address })
});

// ---

declare! {
    IAccountsPskbSendResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsPskbSendResponse {
        transactionIds : HexString[];
    }
    "#,
}

try_from! ( args: AccountsPskbSendResponse, IAccountsPskbSendResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    IAccountsGetUtxosRequest,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsGetUtxosRequest {
        accountId : HexString;
        addresses : Address[] | string[];
        minAmountSompi? : bigint;
    }
    "#,
}

try_from! ( args: IAccountsGetUtxosRequest, AccountsGetUtxosRequest, {
    let account_id = args.get_account_id("accountId")?;
    let addresses = args.try_get_addresses("addresses")?;
    let min_amount_sompi = args.get_u64("minAmountSompi").ok();
    Ok(AccountsGetUtxosRequest { account_id, addresses, min_amount_sompi })
});

declare! {
    IAccountsGetUtxosResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsGetUtxosResponse {
        utxos : UtxoEntry[];
    }
    "#,
}

try_from! ( args: AccountsGetUtxosResponse, IAccountsGetUtxosResponse, {
    let response = IAccountsGetUtxosResponse::default();


    let utxos = args.utxos.into_iter().map(|entry| entry.to_js_object()).collect::<Result<Vec<js_sys::Object>>>()?;
    let utxos = js_sys::Array::from_iter(utxos.into_iter());
    response.set("utxos", &utxos)?;
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
        sourceAccountId : HexString;
        destinationAccountId : HexString;
        walletSecret : string;
        paymentSecret? : string;
        feeRate? : number;
        priorityFeeSompi? : IFees | bigint;
        transferAmountSompi : bigint;
    }
    "#,
}

try_from! ( args: IAccountsTransferRequest, AccountsTransferRequest, {
    let source_account_id = args.get_account_id("sourceAccountId")?;
    let destination_account_id = args.get_account_id("destinationAccountId")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let fee_rate = args.get_f64("feeRate").ok();
    let priority_fee_sompi = args.try_get::<IFees>("priorityFeeSompi")?.map(Fees::try_from).transpose()?;
    let transfer_amount_sompi = args.get_u64("transferAmountSompi")?;

    Ok(AccountsTransferRequest {
        source_account_id,
        destination_account_id,
        wallet_secret,
        payment_secret,
        fee_rate,
        priority_fee_sompi,
        transfer_amount_sompi,
    })
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
        generatorSummary : GeneratorSummary;
        transactionIds : HexString[];
    }
    "#,
}

try_from! ( args: AccountsTransferResponse, IAccountsTransferResponse, {
    let response = IAccountsTransferResponse::default();
    response.set("generatorSummary", &GeneratorSummary::from(args.generator_summary).into())?;
    response.set("transactionIds", &to_value(&args.transaction_ids)?)?;
    Ok(response)
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
        accountId : HexString;
        destination : IPaymentOutput[];
        feeRate? : number;
        priorityFeeSompi : IFees | bigint;
        payload? : Uint8Array | string;
    }
    "#,
}

try_from! ( args: IAccountsEstimateRequest, AccountsEstimateRequest, {
    let account_id = args.get_account_id("accountId")?;
    let fee_rate = args.get_f64("feeRate").ok();
    let priority_fee_sompi = args.get::<IFees>("priorityFeeSompi")?.try_into()?;
    let payload = args.try_get_value("payload")?.map(|v| v.try_as_vec_u8()).transpose()?;

    let outputs = args.get_value("destination")?;
    let destination: PaymentDestination =
        if outputs.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_owned_from(outputs)?.into() };

    Ok(AccountsEstimateRequest { account_id, fee_rate, priority_fee_sompi, destination, payload })
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
        generatorSummary : GeneratorSummary;
    }
    "#,
}

try_from! ( args: AccountsEstimateResponse, IAccountsEstimateResponse, {
    let response = IAccountsEstimateResponse::default();
    response.set("generatorSummary", &GeneratorSummary::from(args.generator_summary).into())?;
    Ok(response)
});

// ---

declare! {
    IFeeRateEstimateBucket,
    r#"
    export interface IFeeRateEstimateBucket {
        feeRate : number;
        seconds : number;
    }
    "#,
}

declare! {
    IFeeRateEstimateRequest,
    r#"
    export interface IFeeRateEstimateRequest { }
    "#,
}

try_from! ( _args: IFeeRateEstimateRequest, FeeRateEstimateRequest, {
    Ok(FeeRateEstimateRequest { })
});

declare! {
    IFeeRateEstimateResponse,
    r#"
    export interface IFeeRateEstimateResponse {
        priority : IFeeRateEstimateBucket,
        normal : IFeeRateEstimateBucket,
        low : IFeeRateEstimateBucket,
    }
    "#,
}

try_from! ( args: FeeRateEstimateResponse, IFeeRateEstimateResponse, {
    Ok(to_value(&args)?.into())
});

declare! {
    IFeeRatePollerEnableRequest,
    r#"
    export interface IFeeRatePollerEnableRequest {
        intervalSeconds : number;
    }
    "#,
}

try_from! ( args: IFeeRatePollerEnableRequest, FeeRatePollerEnableRequest, {
    let interval_seconds = args.get_u64("intervalSeconds")?;
    Ok(FeeRatePollerEnableRequest { interval_seconds })
});

declare! {
    IFeeRatePollerEnableResponse,
    r#"
    export interface IFeeRatePollerEnableResponse { }
    "#,
}

try_from! ( _args: FeeRatePollerEnableResponse, IFeeRatePollerEnableResponse, {
    Ok(IFeeRatePollerEnableResponse::default())
});

declare! {
    IFeeRatePollerDisableRequest,
    r#"
    export interface IFeeRatePollerDisableRequest { }
    "#,
}

try_from! ( _args: IFeeRatePollerDisableRequest, FeeRatePollerDisableRequest, {
    Ok(FeeRatePollerDisableRequest { })
});

declare! {
    IFeeRatePollerDisableResponse,
    r#"
    export interface IFeeRatePollerDisableResponse { }
    "#,
}

try_from! ( _args: FeeRatePollerDisableResponse, IFeeRatePollerDisableResponse, {
    Ok(IFeeRatePollerDisableResponse::default())
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
        accountId : HexString;
        networkId : NetworkId | string;
        filter? : TransactionKind[];
        start : bigint;
        end : bigint;
    }
    "#,
}

try_from! ( args: ITransactionsDataGetRequest, TransactionsDataGetRequest, {
    let account_id = args.get_account_id("accountId")?;
    let network_id = args.get_network_id("networkId")?;
    let filter = args.get_vec("filter").ok().map(|filter| {
        filter.into_iter().map(TransactionKind::try_from).collect::<Result<Vec<TransactionKind>>>()
    }).transpose()?;
    let start = args.get_u64("start")?;
    let end = args.get_u64("end")?;

    let request = TransactionsDataGetRequest {
        account_id,
        network_id,
        filter,
        start,
        end,
    };
    Ok(request)
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
        accountId : HexString;
        transactions : ITransactionRecord[];
        start : bigint;
        total : bigint;
    }
    "#,
}

try_from! ( args: TransactionsDataGetResponse, ITransactionsDataGetResponse, {
    Ok(to_value(&args)?.into())
});

// ---

declare! {
    INetworkParams,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface INetworkParams {
        coinbaseTransactionMaturityPeriodDaa : number;
        coinbaseTransactionStasisPeriodDaa : number;
        userTransactionMaturityPeriodDaa : number;
        additionalCompoundTransactionMass : number;
    }
    "#,
}

try_from! ( args: &NetworkParams, INetworkParams, {
    let response = INetworkParams::default();
    response.set("coinbaseTransactionMaturityPeriodDaa", &to_value(&args.coinbase_transaction_maturity_period_daa)?)?;
    response.set("coinbaseTransactionStasisPeriodDaa", &to_value(&args.coinbase_transaction_stasis_period_daa)?)?;
    response.set("userTransactionMaturityPeriodDaa", &to_value(&args.user_transaction_maturity_period_daa)?)?;
    response.set("additionalCompoundTransactionMass", &to_value(&args.additional_compound_transaction_mass)?)?;
    Ok(response)
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
// ---

declare! {
    IAccountsCommitRevealRequest,
    r#"
    /**
     * 
     * Atomic commit reveal operation using parameterized account address to
     * dynamically generate the commit P2SH address.
     * 
     * The account address is selected through addressType and addressIndex
     * and will be used to complete the script signature.
     * 
     * A placeholder of format {{pubkey}} is to be provided inside ScriptSig
     * in order to be superseded by the selected address' payload.
     * 
     * The selected address will also be used to spend reveal transaction to.
     * 
     * The default revealFeeSompi is 100_000 sompi.
     *  
     * @category Wallet API
     */
    export interface IAccountsCommitRevealRequest {
        accountId : HexString;
        addressType : CommitRevealAddressKind;
        addressIndex : number;
        scriptSig : Uint8Array | HexString;
        walletSecret : string;
        commitAmountSompi : bigint;
        paymentSecret? : string;
        feeRate? : number;
        revealFeeSompi : bigint;
        payload? : Uint8Array | HexString;
    }
    "#,
}

try_from! ( args: IAccountsCommitRevealRequest, AccountsCommitRevealRequest, {
    let account_id = args.get_account_id("accountId")?;
    let address_type = args.get_value("addressType")?;

    let address_type = if let Some(address_type) = address_type.as_string() {
        address_type.parse()?
    } else {
        CommitRevealAddressKind::try_enum_from(&address_type)?
    };

    let address_index = args.get_u32("addressIndex")?;
    let script_sig = args.get_vec_u8("scriptSig")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;
    let commit_amount_sompi = args.get_u64("commitAmountSompi")?;
    let fee_rate = args.get_f64("feeRate").ok();

    let reveal_fee_sompi = args.get_u64("revealFeeSompi")?;

    let payload = args.try_get_value("payload")?.map(|v| v.try_as_vec_u8()).transpose()?;

    Ok(AccountsCommitRevealRequest {
        account_id,
        address_type,
        address_index,
        script_sig,
        wallet_secret,
        payment_secret,
        commit_amount_sompi,
        fee_rate,
        reveal_fee_sompi,
        payload,
    })
});

declare! {
    IAccountsCommitRevealResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCommitRevealResponse {
        transactionIds : HexString[];
    }
    "#,
}

try_from! ( args: AccountsCommitRevealResponse, IAccountsCommitRevealResponse, {
    let response = IAccountsCommitRevealResponse::default();
    response.set("transactionIds", &to_value(&args.transaction_ids)?)?;
    Ok(response)
});

// ---

declare! {
    IAccountsCommitRevealManualRequest,
    r#"
    /**
     * 
     * Atomic commit reveal operation using given payment outputs.
     * 
     * The startDestination stands for the commit transaction and the endDestination
     * for the reveal transaction.
     * 
     * The scriptSig will be used to spend the UTXO of the first transaction and
     * must therefore match the startDestination output P2SH.
     * 
     * Set revealFeeSompi or reflect the reveal fee transaction on endDestination
     * output amount. 
     * 
     * The default revealFeeSompi is 100_000 sompi.
     * 
     * @category Wallet API
     */
    export interface IAccountsCommitRevealManualRequest {
        accountId : HexString;
        scriptSig : Uint8Array | HexString;
        startDestination: IPaymentOutput;
        endDestination: IPaymentOutput;
        walletSecret : string;
        paymentSecret? : string;
        feeRate? : number;
        revealFeeSompi : bigint;
        payload? : Uint8Array | HexString;
    }
    "#,
}

try_from! ( args: IAccountsCommitRevealManualRequest, AccountsCommitRevealManualRequest, {
    let account_id = args.get_account_id("accountId")?;
    let script_sig = args.get_vec_u8("scriptSig")?;
    let wallet_secret = args.get_secret("walletSecret")?;
    let payment_secret = args.try_get_secret("paymentSecret")?;

    let commit_output = args.get_value("startDestination")?;
    let start_destination: PaymentDestination =
    if commit_output.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_owned_from(commit_output)?.into() };

    let reveal_output = args.get_value("endDestination")?;
    let end_destination: PaymentDestination =
    if reveal_output.is_undefined() { PaymentDestination::Change } else { PaymentOutputs::try_owned_from(reveal_output)?.into() };

    let fee_rate = args.get_f64("feeRate").ok();
    let reveal_fee_sompi = args.get_u64("revealFeeSompi")?;

    let payload = args.try_get_value("payload")?.map(|v| v.try_as_vec_u8()).transpose()?;

    Ok(AccountsCommitRevealManualRequest {
        account_id,
        script_sig,
        wallet_secret,
        payment_secret,
        start_destination,
        end_destination,
        fee_rate,
        reveal_fee_sompi,
        payload,
    })
});

declare! {
    IAccountsCommitRevealManualResponse,
    r#"
    /**
     * 
     *  
     * @category Wallet API
     */
    export interface IAccountsCommitRevealManualResponse {
        transactionIds : HexString[];
    }
    "#,
}

try_from! ( args: AccountsCommitRevealManualResponse, IAccountsCommitRevealManualResponse, {
    let response = IAccountsCommitRevealManualResponse::default();
    response.set("transactionIds", &to_value(&args.transaction_ids)?)?;
    Ok(response)
});

// ---
