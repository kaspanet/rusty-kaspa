globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('./kaspa/kaspa_wasm');
const {parseArgs} = require("./utils");
const {
    RpcClient
} = kaspa;
kaspa.init_console_panic_hook();

(async () => {
    const {
        networkType,
        encoding,
    } = parseArgs();

    const rpc = new RpcClient(encoding, "127.0.0.1", networkType);
    console.log(`# connecting to ${rpc.url}`)
    await rpc.connect();

    const info = await rpc.getBlockDagInfo();
    console.log("info:", info);

    await rpc.disconnect();
})();
