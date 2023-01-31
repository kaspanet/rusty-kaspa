globalThis.WebSocket = require('websocket').w3cwebsocket;

let {RpcClient,Encoding,init_console_panic_hook} = require('./kaspa-rpc');

// init_console_panic_hook();

let URL = "ws://127.0.0.1:17110";
let rpc = new RpcClient(Encoding.Borsh,URL);

(async () => {
    console.log(`# connecting to ${URL}`)
    await rpc.connect();
    console.log(`# connected!`)

    // let info = await rpc.get_block_dag_info();
    let info = await rpc.get_info();
    console.log(info);
    await rpc.disconnect();

})();