#![allow(non_snake_case)]
use cfg_if::cfg_if;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use wasm_bindgen::prelude::*;

cfg_if! {
    if #[cfg(any(feature = "wasm32-core", feature = "wasm32-sdk"))] {

        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"

        /**
         * Events emitted by the {@link UtxoProcessor}.
         * @category Wallet SDK
         */
        export enum UtxoProcessorEventType {
            Connect = "connect",
            Disconnect = "disconnect",
            UtxoIndexNotEnabled = "utxo-index-not-enabled",
            SyncState = "sync-state",
            UtxoProcStart = "utxo-proc-start",
            UtxoProcStop = "utxo-proc-stop",
            UtxoProcError = "utxo-proc-error",
            DaaScoreChange = "daa-score-change",
            Pending = "pending",
            Reorg = "reorg",
            Stasis = "stasis",
            Maturity = "maturity",
            Discovery = "discovery",
            Balance = "balance",
            Error = "error",
        }

        /**
         * {@link UtxoProcessor} notification event data.
         * @category Wallet SDK
         */
        export type UtxoProcessorEventData = IConnectEvent
            | IDisconnectEvent
            | IUtxoIndexNotEnabledEvent
            | ISyncStateEvent
            | IServerStatusEvent
            | IUtxoProcErrorEvent
            | IDaaScoreChangeEvent
            | IPendingEvent
            | IReorgEvent
            | IStasisEvent
            | IMaturityEvent
            | IDiscoveryEvent
            | IBalanceEvent
            | IErrorEvent
            | undefined
            ;

        /**
         * UtxoProcessor notification event data map.
         * 
         * @category Wallet API
         */
        export type UtxoProcessorEventMap = {
            "connect":IConnectEvent,
            "disconnect": IDisconnectEvent,
            "utxo-index-not-enabled": IUtxoIndexNotEnabledEvent,
            "sync-state": ISyncStateEvent,
            "server-status": IServerStatusEvent,
            "utxo-proc-start": undefined,
            "utxo-proc-stop": undefined,
            "utxo-proc-error": IUtxoProcErrorEvent,
            "daa-score-change": IDaaScoreChangeEvent,
            "pending": IPendingEvent,
            "reorg": IReorgEvent,
            "stasis": IStasisEvent,
            "maturity": IMaturityEvent,
            "discovery": IDiscoveryEvent,
            "balance": IBalanceEvent,
            "error": IErrorEvent
        }

        /**
         * 
         * @category Wallet API
         */
        export type IUtxoProcessorEvent = {
            [K in keyof UtxoProcessorEventMap]: { event: K, data: UtxoProcessorEventMap[K] }
        }[keyof UtxoProcessorEventMap];

        
        /**
         * {@link UtxoProcessor} notification callback type.
         * 
         * This type declares the callback function that is called when notification is emitted
         * from the UtxoProcessor or UtxoContext subsystems.
         * 
         * @see {@link UtxoProcessor}, {@link UtxoContext},
         * 
         * @category Wallet SDK
         */
        export type UtxoProcessorNotificationCallback = (event: IUtxoProcessorEvent) => void;
        "#;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(typescript_type = "UtxoProcessorEventType | UtxoProcessorEventType[] | string | string[]")]
            pub type UtxoProcessorEventTarget;
            #[wasm_bindgen(extends = js_sys::Function, typescript_type = "UtxoProcessorNotificationCallback")]
            pub type UtxoProcessorNotificationCallback;
            #[wasm_bindgen(extends = js_sys::Function, typescript_type = "string | UtxoProcessorNotificationCallback")]
            pub type UtxoProcessorNotificationTypeOrCallback;
        }
    }
}

cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"

        /**
         * Events emitted by the {@link Wallet}.
         * @category Wallet API
         */
        export enum WalletEventType {
            Connect = "connect",
            Disconnect = "disconnect",
            UtxoIndexNotEnabled = "utxo-index-not-enabled",
            SyncState = "sync-state",
            WalletHint = "wallet-hint",
            WalletOpen = "wallet-open",
            WalletCreate = "wallet-create",
            WalletReload = "wallet-reload",
            WalletError = "wallet-error",
            WalletClose = "wallet-close",
            PrvKeyDataCreate = "prv-key-data-create",
            AccountActivation = "account-activation",
            AccountDeactivation = "account-deactivation",
            AccountSelection = "account-selection",
            AccountCreate = "account-create",
            AccountUpdate = "account-update",
            ServerStatus = "server-status",
            UtxoProcStart = "utxo-proc-start",
            UtxoProcStop = "utxo-proc-stop",
            UtxoProcError = "utxo-proc-error",
            DaaScoreChange = "daa-score-change",
            Pending = "pending",
            Reorg = "reorg",
            Stasis = "stasis",
            Maturity = "maturity",
            Discovery = "discovery",
            Balance = "balance",
            Error = "error",
        }


        /**
         * {@link Wallet} notification event data payload.
         * @category Wallet API
         */
        export type WalletEventData = IConnectEvent
            | IDisconnectEvent
            | IUtxoIndexNotEnabledEvent
            | ISyncStateEvent
            | IWalletHintEvent
            | IWalletOpenEvent
            | IWalletCreateEvent
            | IWalletReloadEvent
            | IWalletErrorEvent
            // | IWalletCloseEvent
            | IPrvKeyDataCreateEvent
            | IAccountActivationEvent
            | IAccountDeactivationEvent
            | IAccountSelectionEvent
            | IAccountCreateEvent
        | IAccountUpdateEvent
            | IServerStatusEvent
            // | IUtxoProcStartEvent
            // | IUtxoProcStopEvent
            | IUtxoProcErrorEvent
            | IDaaScoreChangeEvent
            | IPendingEvent
            | IReorgEvent
            | IStasisEvent
            | IMaturityEvent
            | IDiscoveryEvent
            | IBalanceEvent
            | IErrorEvent
            | undefined
            ;

        /**
         * Wallet notification event data map.
         * @see {@link Wallet.addEventListener}
         * @category Wallet API
         */
        export type WalletEventMap = {
             "connect": IConnectEvent,
             "disconnect": IDisconnectEvent,
             "utxo-index-not-enabled": IUtxoIndexNotEnabledEvent,
             "sync-state": ISyncStateEvent,
             "wallet-hint": IWalletHintEvent,
             "wallet-open": IWalletOpenEvent,
             "wallet-create": IWalletCreateEvent,
             "wallet-reload": IWalletReloadEvent,
             "wallet-error": IWalletErrorEvent,
             "wallet-close": undefined,
             "prv-key-data-create": IPrvKeyDataCreateEvent,
             "account-activation": IAccountActivationEvent,
             "account-deactivation": IAccountDeactivationEvent,
             "account-selection": IAccountSelectionEvent,
             "account-create": IAccountCreateEvent,
             "account-update": IAccountUpdateEvent,
             "server-status": IServerStatusEvent,
             "utxo-proc-start": undefined,
             "utxo-proc-stop": undefined,
             "utxo-proc-error": IUtxoProcErrorEvent,
             "daa-score-change": IDaaScoreChangeEvent,
             "pending": IPendingEvent,
             "reorg": IReorgEvent,
             "stasis": IStasisEvent,
             "maturity": IMaturityEvent,
             "discovery": IDiscoveryEvent,
             "balance": IBalanceEvent,
             "error": IErrorEvent,
        }
        
        /**
         * {@link Wallet} notification event interface.
         * @category Wallet API
         */
        export type IWalletEvent = {
            [K in keyof WalletEventMap]: { type: K, data: WalletEventMap[K] }
        }[keyof WalletEventMap];

        /**
         * Wallet notification callback type.
         * 
         * This type declares the callback function that is called when notification is emitted
         * from the Wallet (and the underlying UtxoProcessor or UtxoContext subsystems).
         * 
         * @see {@link Wallet}
         * 
         * @category Wallet API
         */
        export type WalletNotificationCallback = (event: IWalletEvent) => void;
        "#;

        #[wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(typescript_type = "WalletEventType | WalletEventType[] | string | string[]")]
            pub type WalletEventTarget;
            #[wasm_bindgen(extends = js_sys::Function, typescript_type = "WalletNotificationCallback")]
            pub type WalletNotificationCallback;
            #[wasm_bindgen(extends = js_sys::Function, typescript_type = "string | WalletNotificationCallback")]
            pub type WalletNotificationTypeOrCallback;
        }
    }
}

declare! {
    IConnectEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when it negotiates a successful RPC connection.
     * 
     * @category Wallet Events
     */
    export interface IConnectEvent {
        networkId : string;
        url? : string;
    }
    "#,
}

declare! {
    IDisconnectEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when it disconnects from RPC.
     * 
     * @category Wallet Events
     */
    export interface IDisconnectEvent {
        networkId : string;
        url? : string;
    }
    "#,
}

declare! {
    IUtxoIndexNotEnabledEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when it detects that connected node does not have UTXO index enabled.
     * 
     * @category Wallet Events
     */
    export interface IUtxoIndexNotEnabledEvent {
        url? : string;
    }
    "#,
}

declare! {
    ISyncStateEvent,
    r#"

    /**
     * 
     * @category Wallet Events
     */
    export interface ISyncState {
        event : string;
        data? : ISyncProofEvent | ISyncHeadersEvent | ISyncBlocksEvent | ISyncUtxoSyncEvent | ISyncTrustSyncEvent;
    }
    
    /**
     * 
     * @category Wallet Events
     */
    export interface ISyncStateEvent {
        syncState : ISyncState;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IWalletHintEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when it opens and contains an optional anti-phishing 'hint' set by the user.
     * 
     * @category Wallet Events
     */
    export interface IWalletHintEvent {
        hint? : string;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IWalletOpenEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when the wallet is successfully opened.
     * 
     * @category Wallet Events
     */
    export interface IWalletOpenEvent {
        walletDescriptor : IWalletDescriptor;
        accountDescriptors : IAccountDescriptor[];
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IWalletCreateEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when the wallet data storage has been successfully created.
     * 
     * @category Wallet Events
     */
    export interface IWalletCreateEvent {
        walletDescriptor : IWalletDescriptor;
        storageDescriptor : IStorageDescriptor;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IWalletReloadEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when the wallet is successfully reloaded.
     * 
     * @category Wallet Events
     */
    export interface IWalletReloadEvent {
        walletDescriptor : IWalletDescriptor;
        accountDescriptors : IAccountDescriptor[];
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IWalletErrorEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an error occurs (for example, the wallet has failed to open).
     * 
     * @category Wallet Events
     */
    export interface IWalletErrorEvent {
        message : string;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IPrvKeyDataCreateEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when the wallet has created a private key.
     * 
     * @category Wallet Events
     */
    export interface IPrvKeyDataCreateEvent {
        prvKeyDataInfo : IPrvKeyDataInfo;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IAccountActivationEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an account has been activated.
     * 
     * @category Wallet Events
     */
    export interface IAccountActivationEvent {
        ids : HexString[];
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IAccountDeactivationEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an account has been deactivated.
     * 
     * @category Wallet Events
     */
    export interface IAccountDeactivationEvent {
        ids : HexString[];
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IAccountSelectionEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an account has been selected.
     * This event is used internally in Rust SDK to track currently
     * selected account in the Rust CLI wallet.
     * 
     * @category Wallet Events
     */
    export interface IAccountSelectionEvent {
        id? : HexString;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IAccountCreateEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an account has been created.
     * 
     * @category Wallet Events
     */
    export interface IAccountCreateEvent {
        accountDescriptor : IAccountDescriptor;
    }
    "#,
}

#[cfg(feature = "wasm32-sdk")]
declare! {
    IAccountUpdateEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when an account data has been updated.
     * This event signifies a chance in the internal account state that
     * includes new address generation.
     * 
     * @category Wallet Events
     */
    export interface IAccountUpdateEvent {
        accountDescriptor : IAccountDescriptor;
    }
    "#,
}

declare! {
    IServerStatusEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} after successfully opening an RPC
     * connection to the Kaspa node. This event contains general information
     * about the Kaspa node.
     * 
     * @category Wallet Events
     */
    export interface IServerStatusEvent {
        networkId : string;
        serverVersion : string;
        isSynced : boolean;
        url? : string;
    }
    "#,
}

declare! {
    IUtxoProcErrorEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} indicating a non-recoverable internal error.
     * If such event is emitted, the application should stop the UtxoProcessor
     * and restart all related subsystem. This event is emitted when the UtxoProcessor
     * encounters a critical condition such as "out of memory".
     * 
     * @category Wallet Events
     */
    export interface IUtxoProcErrorEvent {
        message : string;
    }
    "#,
}

declare! {
    IDaaScoreChangeEvent,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} on DAA score change.
     * 
     * @category Wallet Events
     */
    export interface IDaaScoreChangeEvent {
        currentDaaScore : number;
    }
    "#,
}

declare! {
    IPendingEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when detecting a pending transaction.
     * This notification will be followed by the "balance" event.
     * 
     * @category Wallet Events
     */
    export type IPendingEvent = TransactionRecord;
    "#,
}

declare! {
    IReorgEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when detecting a reorg transaction condition.
     * A transaction is considered reorg if it has been removed from the UTXO set
     * as a part of the network reorg process. Transactions notified with this event
     * should be considered as invalid and should be removed from the application state.
     * Associated UTXOs will be automatically removed from the UtxoContext state.
     * 
     * @category Wallet Events
     */
    export type IReorgEvent = TransactionRecord;
    "#,
}

declare! {
    IStasisEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when detecting a new coinbase transaction.
     * Transactions are kept in "stasis" for the half of the coinbase maturity DAA period.
     * A wallet should ignore these transactions until they are re-broadcasted
     * via the "pending" event.
     * 
     * @category Wallet Events
     */
    export type IStasisEvent = TransactionRecord;
    "#,
}

declare! {
    IMaturityEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when transaction is considered to be confirmed.
     * This notification will be followed by the "balance" event.
     * 
     * @category Wallet Events
     */
    export type IMaturityEvent = TransactionRecord;
    "#,
}

declare! {
    IDiscoveryEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when detecting a new transaction during
     * the initialization phase. Discovery transactions indicate that UTXOs
     * have been discovered during the initial UTXO scan.
     * 
     * When receiving such notifications, the application should check its 
     * internal storage to see if the transaction already exists. If it doesn't,
     * it should create a correspond in record and notify the user of a new
     * transaction.
     * 
     * This event is emitted when an address has existing UTXO entries that
     * may have been received during previous sessions or while the wallet
     * was offline.
     * 
     * @category Wallet Events
     */
    export type IDiscoveryEvent = TransactionRecord;
    "#,
}

declare! {
    IBalanceEvent,
    r#"
    /**
     * Emitted by {@link UtxoContext} when detecting a balance change.
     * This notification is produced during the UTXO scan, when UtxoContext
     * detects incoming or outgoing transactions or when transactions
     * change their state (e.g. from pending to confirmed).
     * 
     * @category Wallet Events
     */
    export interface IBalanceEvent {
        id : HexString;
        balance? : IBalance;
    }
    "#,
}

declare! {
    IErrorEvent,
    r#"
    /**
     * Emitted when detecting a general error condition.
     * 
     * @category Wallet Events
     */
    export interface IErrorEvent {
        message : string;
    }
    "#,
}

// ---

declare! {
    ISyncProof,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when node is syncing and processing cryptographic proofs.
     * 
     * @category Wallet Events
     */
    export interface ISyncProofEvent {
        level : number;
    }
    "#,
}

declare! {
    ISyncHeaders,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when node is syncing headers as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncHeadersEvent {
        headers : number;
        progress : number;
    }
    "#,
}

declare! {
    ISyncBlocks,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when node is syncing blocks as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncBlocksEvent {
        blocks : number;
        progress : number;
    }
    "#,
}

declare! {
    ISyncUtxoSync,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when node is syncing the UTXO set as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncUtxoSyncEvent {
        chunks : number;
        total : number;
    }
    "#,
}

declare! {
    ISyncTrustSync,
    r#"
    /**
     * Emitted by {@link UtxoProcessor} when node is syncing cryptographic trust data as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncTrustSyncEvent {
        processed : number;
        total : number;
    }
    "#,
}
