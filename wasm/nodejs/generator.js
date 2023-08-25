// Run with: node demo.js
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    Encoding,
    NetworkType,
    UtxoEntries,
    kaspaToSompi,
    createTransactions,
    initConsolePanicHook
} = require('./kaspa/kaspa_wasm');

initConsolePanicHook();

(async () => {

    let args = process.argv.slice(2);
    let destination = args.shift() || "kaspatest:qqkl0ct62rv6dz74pff2kx5sfyasl4z28uekevau23g877r5gt6userwyrmtt";
    console.log("using destination address:", destination);

    // ---
    // network type
    let network = NetworkType.Testnet;
    // RPC encoding
    let encoding = Encoding.Borsh;
    // ---

    // From BIP0340
    const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');

    const address = sk.toKeypair().toAddress(network);
    // Full kaspa address: kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    console.info(`Kaspa address: ${address}`);

    let rpcUrl = RpcClient.parseUrl("127.0.0.1", encoding, network);
    const rpc = new RpcClient(encoding, rpcUrl, network);
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }


    let entries = await rpc.getUtxosByAddresses([address]);

    if (!entries.length) {
        console.error(`No UTXOs found for address ${address}`);
    } else {
        console.info(entries);

        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.utxoEntry.amount > b.utxoEntry.amount || -(a.utxoEntry.amount < b.utxoEntry.amount));

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
        // transactoin mass, the Generator will create multiple
        // transactions where each transaction will forward
        // UTXOs to the change address, until the requested
        // amount is reached.  It will then create a final
        // transaction according to the supplied outputs.
        let generator = new Generator({
            entries,
            outputs: [[destination, kaspaToSompi(0.2)]],
            priorityFee: 0,
            changeAddress: address,
        });

        // transaction generator creates a 
        // sequence of transactions
        // for a requested amount of KAS.
        // sign and submit these transactions
        while (pending = await generator.next()) {
            await pending.sign([privateKey]);
            let txid = await pending.submit(rpc);
            console.log("txid:", txid);
        }

        console.log("summary:", generator.summary());

    }

    await rpc.disconnect();

})();
