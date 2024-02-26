// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    Generator,
    UtxoProcessor,
    UtxoContext,
    kaspaToSompi,
    createTransactions,
    initConsolePanicHook
} = require('../../../../nodejs/kaspa');

initConsolePanicHook();

const { encoding, networkId, address : destinationAddress } = require("../utils").parseArgs();


(async () => {

    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    console.log(`Source address: ${sourceAddress}`);

    // if not destination is specified, send back to ourselves
    let address = destinationAddress ?? sourceAddress;
    console.log(`Destination address: ${destinationAddress}`);

    // 1) Initialize RPC
    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });

    // 2) Create UtxoProcessor, passing RPC to it
    let processor = new UtxoProcessor({ rpc, networkId });
    await processor.start();

    // 3) Create one of more UtxoContext, passing UtxoProcessor to it
    // you can create UtxoContext objects as needed to monitor different
    // address sets.
    let context = await new UtxoContext({ processor });

    // 4) Register a listener with the UtxoProcessor::events
    processor.addEventListener((event) => {
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

    // 7) Check balance, if there are enough funds, send a transaction
    if (context.balance.mature > kaspaToSompi(0.2) + 1000n) {
        console.log("Sending transaction");

        let generator = new Generator({
            entries : context,
            outputs: [{address, amount : kaspaToSompi(0.2)}],
            priorityFee: kaspaToSompi(0.0001),
            changeAddress: sourceAddress,
        });

        let pending;
        while (pending = await generator.next()) {
            await pending.sign([privateKey]);
            let txid = await pending.submit(rpc);
            console.log("txid:", txid);
        }

        console.log("summary:", generator.summary());

    } else {
        console.log("Not enough funds to send transaction");
    }

    await processor.shutdown();
    await rpc.disconnect();

})();