globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../kaspa/kaspa_wasm');
const {parseArgs} = require("../utils");
kaspa.init_console_panic_hook();

(async () => {
    const {} = parseArgs();

    const iter = kaspa.get_async_iter();
    console.log("iter ->", iter);
    for await (const item of iter) {
        console.log("item ->", item);
    }

    // let URL = "ws://127.0.0.1:17110";
    // let rpc = new RpcClient(Encoding.Borsh,URL);

    // console.log(`# connecting to ${URL}`)
    // await rpc.connect();

    // let info = await rpc.getBlockDagInfo();
    // console.log("info:", info);

    // await rpc.disconnect();

})();
