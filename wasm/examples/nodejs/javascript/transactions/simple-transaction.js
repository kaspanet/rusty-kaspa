// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    kaspaToSompi,
    createTransactions,
    initConsolePanicHook
} = require('../../../../nodejs/kaspa');

const { encoding, networkId, address: destinationAddressArg } = require("../utils").parseArgs();

initConsolePanicHook();

(async () => {


    // From BIP0340
    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');

    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    console.info(`Source address: ${sourceAddress}`);

    // if not destination address is supplied, send funds to source address
    const destinationAddress = destinationAddressArg || sourceAddress;
    console.log(`Destination address: ${destinationAddress}`);

    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    let { isSynced, virtualDaaScore } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    let { entries } = (await rpc.getUtxosByAddresses([sourceAddress]));

    if (!entries.length) {
        console.error("No UTXOs found for address");
    } else {
        console.info(entries);

        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.amount > b.amount ? 1 : -1);

        let { transactions, summary } = await createTransactions({
            entries,
            outputs: [{ address : destinationAddress, amount : kaspaToSompi("0.00012")}],
            priorityFee: 0n,
            changeAddress: sourceAddress,
        });

        console.log("Summary:", summary);

        for (let pending of transactions) {
            console.log("Pending transaction:", pending);
            console.log("Signing tx with secret key:", privateKey.toString());
            await pending.sign([privateKey]);
            console.log("Submitting pending tx to RPC ...")
            let txid = await pending.submit(rpc);
            console.log("Node responded with txid:", txid);
        }
    }

    await rpc.disconnect();

})();