// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../../../../nodejs/kaspa');
const { parseArgs } = require("../utils");
const {
    RpcClient,
    Resolver,
    // RpcEventType
} = kaspa;

kaspa.initConsolePanicHook();

const {
    networkId,
    encoding,
} = parseArgs();

(async () => {

    const rpc = new RpcClient({
        resolver: new Resolver(),
        networkId
    });

    console.log("Registering for DAA notifications...");
    rpc.addEventListener("virtual-daa-score-changed", async (event) => {
        console.log(event);
    });
    
    console.log("Registering for RPC online event...");
    rpc.addEventListener("connect", async (event) => {
        console.log("Connected to", rpc.url);
        console.log(event);
        console.log("Subscribing to DAA score...");
        rpc.subscribeVirtualDaaScoreChanged();
    });

    console.log(`Connecting...`);
    await rpc.connect();

    process.on('SIGINT', async () => {
        console.log('SIGINT');
        console.log("Disconnecting...");
        await rpc.disconnect();
        console.log("Disconnected...");
        console.log("bye!");
        process.exit(0);
    });

})();
