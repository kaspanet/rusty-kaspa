globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('./kaspa/kaspa_wasm');
let { RpcClient, 
    Encoding, NetworkType
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    
    let rpc = new RpcClient(Encoding.Borsh,"127.0.0.1", NetworkType.Testnet);
    console.log(`# connecting to ${rpc.url}`)
    await rpc.connect();

    let info = await rpc.getBlockDagInfo();
    console.log("info:", info);
    
    await rpc.disconnect();

})();