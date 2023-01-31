// W3C WebSocket module shim
globalThis.WebSocket = require('websocket').w3cwebsocket;

let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa-rpc');
// init_console_panic_hook();

const MAX_NOTIFICATION = 3;
let URL = "ws://127.0.0.1:17110";
let rpc = new RpcClient(Encoding.Borsh,URL);

(async () => {
    console.log(`# connecting to ${URL}`)
    await rpc.connect();
    console.log(`# connected ...`)

    let info = await rpc.getInfo();
    console.log(info);
    
    let finish = defer();
    let seq = 0;
    await rpc.notify(async (op, payload) => {
        console.log("op:",op,"payload:",payload);
        seq++;
        if (seq == MAX_NOTIFICATION) {
            // await rpc.disconnect();
            console.log(`exiting after ${seq} notifications`);
            finish.resolve();
        }
    });

    console.log("subscribing...");
    await rpc.subscribeDaaScore();

    await finish;

    await rpc.notify(null);

    await rpc.disconnect();

})();