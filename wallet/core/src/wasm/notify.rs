#![allow(non_snake_case)]
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEADER: &'static str = r#"
/**
 * Wallet notification data payload.
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
    | IPrvDataCreateEvent
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
    ;

/**
 * Wallet notification event interface.
 */
export interface IWalletEvent {
    event : string;
    data? : WalletEventData;
}

/**
 * Wallet notification callback type.
 * 
 * This type declares the callback function that is called when notification is emitted
 * from the Wallet, UtxoProcessor or UtxoContext subsystems.
 * 
 * @see {@link Wallet}, {@link UtxoProcessor}, {@link UtxoContext},
 * 
 * @category Wallet SDK
 */
export type WalletNotificationCallback = (event: IWalletEvent) => void;
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "WalletNotificationCallback")]
    pub type WalletNotificationCallback;
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

declare! {
    IPrvDataCreateEvent,
    r#"
    /**
     * Emitted by {@link Wallet} when the wallet has created a private key.
     * 
     * @category Wallet Events
     */
    export interface IPrvDataCreateEvent {
        prvKeyDataInfo : IPrvKeyDataInfo;
    }
    "#,
}

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
    export interface IPendingEvent {
        record : ITransactionRecord;
    }
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
    export interface IReorgEvent {
        record : ITransactionRecord;
    }
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
    export interface IStasisEvent {
        record : ITransactionRecord;
    }
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
    export interface IMaturityEvent {
        record : ITransactionRecord;
    }
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
    export interface IDiscoveryEvent {
        record : ITransactionRecord;
    }
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