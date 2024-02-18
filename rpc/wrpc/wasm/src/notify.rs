use crate::imports::*;
use kaspa_rpc_macros::declare_typescript_wasm_interface as declare;

#[wasm_bindgen(typescript_custom_section)]
const TS_HEADER: &'static str = r#"

/**
 * RPC notification data payload.
 */

export type RpcNotification = IBlockAdded 
    | IVirtualChainChanged 
    | IFinalityConflict 
    | IFinalityConflictResolved 
    | IUtxosChanged 
    | ISinkBlueScoreChanged 
    | IVirtualDaaScoreChanged 
    | IPruningPointUtxoSetOverride 
    | INewBlockTemplate;

/**
 * RPC notification callback type.
 * 
 * This type is used to define the callback function that is called when an RPC notification is received.
 * 
 * @see {@link RpcClient.subscribeDaaScore},
 * {@link RpcClient.subscribeUtxosChanged}, 
 * {@link RpcClient.subscribeVirtualChainChanged},
 * {@link RpcClient.subscribeBlockAdded},
 * {@link RpcClient.subscribeFinalityConflict},
 * {@link RpcClient.subscribeFinalityConflictResolved},
 * {@link RpcClient.subscribeSinkBlueScoreChanged},
 * {@link RpcClient.subscribePruningPointUtxoSetOverride},
 * {@link RpcClient.subscribeNewBlockTemplate},
 * 
 * @category Node RPC
 */
export type RpcNotificationCallback = (event : string, data: RpcNotification) => void;
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = js_sys::Function, typescript_type = "RpcNotificationCallback")]
    pub type RpcNotificationCallback;
}

declare! {
    IBlockAdded,
    r#"
    /**
     * Block added notification event is produced when a new
     * block is added to the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IBlockAdded {
        [key: string]: any;
    }
    "#,
}

declare! {
    IVirtualChainChanged,
    r#"
    /**
     * Virtual chain changed notification event is produced when the virtual
     * chain changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualChainChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IFinalityConflict,
    r#"
    /**
     * Finality conflict notification event is produced when a finality
     * conflict occurs in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflict {
        [key: string]: any;
    }
    "#,
}

declare! {
    IFinalityConflictResolved,
    r#"
    /**
     * Finality conflict resolved notification event is produced when a finality
     * conflict in the Kaspa BlockDAG is resolved.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflictResolved {
        [key: string]: any;
    }
    "#,
}

declare! {
    IUtxosChanged,
    r#"
    /**
     * UTXOs changed notification event is produced when the set
     * of unspent transaction outputs (UTXOs) changes in the
     * Kaspa BlockDAG. The event notification is scoped to the
     * monitored list of addresses specified during the subscription.
     * 
     * @category Node RPC
     */
    export interface IUtxosChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    ISinkBlueScoreChanged,
    r#"
    /**
     * Sink blue score changed notification event is produced when the blue
     * score of the sink block changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface ISinkBlueScoreChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IVirtualDaaScoreChanged,
    r#"
    /**
     * Virtual DAA score changed notification event is produced when the virtual
     * Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualDaaScoreChanged {
        [key: string]: any;
    }
    "#,
}

declare! {
    IPruningPointUtxoSetOverride,
    r#"
    /**
     * Pruning point UTXO set override notification event is produced when the
     * UTXO set override for the pruning point changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IPruningPointUtxoSetOverride {
        [key: string]: any;
    }
    "#,
}

declare! {
    INewBlockTemplate,
    r#"
    /**
     * New block template notification event is produced when a new block
     * template is generated for mining in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface INewBlockTemplate {
        [key: string]: any;
    }
    "#,
}
