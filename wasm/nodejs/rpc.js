globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('./kaspa/kaspa_wasm');
let { RpcClient, 
    Encoding, 
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    
    let URL = "ws://127.0.0.1:17110";
    let rpc = new RpcClient(Encoding.Borsh,URL);
    
    console.log(`# connecting to ${URL}`)
    await rpc.connect();

    let info = await rpc.getBlockDagInfo();
    console.log("info:", info);
    
    await rpc.disconnect();

})();