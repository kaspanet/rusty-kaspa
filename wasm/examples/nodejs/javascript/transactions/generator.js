// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    RpcClient,
    Generator,
    kaspaToSompi,
    initConsolePanicHook
} = require('../../../../nodejs/kaspa');

initConsolePanicHook();

const { encoding, networkId, address : destinationAddress } = require("../utils").parseArgs();

(async () => {

    // From BIP0340
    const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');

    const sourceAddress = privateKey.toKeypair().toAddress(networkId);
    console.info(`Source address: ${sourceAddress}`);

    // if not destination address is supplied, send funds to source address
    let address = destinationAddress || sourceAddress;
    console.info(`Destination address: ${address}`);

    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }


    let { entries } = await rpc.getUtxosByAddresses([sourceAddress]);

    if (!entries.length) {
        console.error(`No UTXOs found for address ${sourceAddress}`);
    } else {
        console.info(entries);

        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.amount > b.amount ? 1 : -1);

        // create a transaction generator
        // entries: an array of UtxoEntry
        // outputs: an array of [address, amount]
        //
        // priorityFee: a priodityFee value in Sompi
        // NOTE: The priorityFee applies only to the final transaction
        //
        // changeAddress: a change address
        //
        // NOTE: the Generator iterate over the entries array
        // and create transactions until the requested amount
        // is reached. The remaining amount will be sent 
        // to the change address.
        //
        // If the requested amount is greater than the Kaspa
        // transaction mass, the Generator will create multiple
        // transactions where each transaction will forward
        // UTXOs to the change address, until the requested
        // amount is reached.  It will then create a final
        // transaction according to the supplied outputs.
        let generator = new Generator({
            entries,
            outputs: [{address, amount : kaspaToSompi(0.2)}],
            // priorityFee: 1000n,
            priorityFee: kaspaToSompi(0.0001),
            changeAddress: sourceAddress,
        });

        // transaction generator creates a 
        // sequence of transactions
        // for a requested amount of KAS.
        // sign and submit these transactions
        let pending;
        while (pending = await generator.next()) {
            await pending.sign([privateKey]);
            let txid = await pending.submit(rpc);
            console.log("txid:", txid);
        }

        console.log("summary:", generator.summary());

    }

    await rpc.disconnect();

})();
