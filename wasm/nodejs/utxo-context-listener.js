// Run with: node demo.js
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    UtxoProcessor,
    UtxoContext,
    kaspaToSompi,
    createTransactions,
    initConsolePanicHook
} = require('./kaspa/kaspa_wasm');

initConsolePanicHook();

const { encoding, networkId, destinationAddress } = require("./utils").parseArgs();

(async () => {

    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const sourceAddress = privateKey.toKeypair().toAddress(networkType);
    console.info(`Source address: ${sourceAddress}`);

    // if not destination is specified, send back to ourselves
    destinationAddress = destinationAddress ?? sourceAddress;
    console.info(`Destination address: ${destinationAddress}`);

    // 1) Initialize RPC
    const rpc = new RpcClient("127.0.0.1", encoding, networkType);

    // 2) Create UtxoProcessor, passing RPC to it
    let processor = await new UtxoProcessor({ rpc, networkId });

    // 3) Create one of more UtxoContext, passing UtxoProcessor to it
    // you can create UtxoContext objects as needed to monitor different
    // address sets.
    let context = await new UtxoContext({ processor });

    // 4) Register a listener with the UtxoProcessor::events
    processor.events.registerListener((event) => {
        console.log("event:", event);
    });

    console.log(processor);

    // 5) Once the environment is setup, connect to RPC
    console.log(`Connecting to ${rpc.url}`);
    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    // 6) Register the address list with the UtxoContext
    await context.trackAddresses([sourceAddress]);

})();