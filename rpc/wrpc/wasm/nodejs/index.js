// W3C WebSocket module shim
globalThis.WebSocket = require('websocket').w3cwebsocket;

let {RpcClient,Encoding,initConsolePanicHook,defer} = require('./kaspa-rpc');
initConsolePanicHook();

const MAX_NOTIFICATION = 10;
let url = "ws://127.0.0.1:17110";
let rpc = new RpcClient({
    url,
    encoding : Encoding.Borsh,
});

(async () => {
    console.log(`# connecting to ${url}`)
    await rpc.connect();
    console.log(`# connected ...`)

    let info = await rpc.getInfo();
    console.log(info);
    
    let finish = defer();
    let seq = 0;
    // register notification handler
    await rpc.registerListener(async (op, payload) => {
        console.log(`#${seq} - `,"op:",op,"payload:",payload);
        seq++;
        if (seq == MAX_NOTIFICATION) {
            // await rpc.disconnect();
            console.log(`exiting after ${seq} notifications`);
            finish.resolve();
        }
    });

    // test subscription
    console.log("subscribing...");
    await rpc.subscribeDaaScore();

    // wait until notifier signals completion
    await finish;
    // clear notification handler
    await rpc.removeListener();
    // disconnect RPC interface
    await rpc.disconnect();

})();