// Run with: node demo.js
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    kaspaToSompi,
    createTransactions,
    initConsolePanicHook
} = require('./kaspa/kaspa_wasm');
const { parseArgs } = require('./utils');

initConsolePanicHook();

(async ()=>{

    let {
        address: destinationAddress,
        networkType,
        encoding,
    } = parseArgs();
    destinationAddress = destinationAddress ?? "kaspatest:qqkl0ct62rv6dz74pff2kx5sfyasl4z28uekevau23g877r5gt6userwyrmtt";
    console.log("using destination address:", destinationAddress);

    // From BIP0340
    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');

    const kaspaAddress = privateKey.toKeypair().toAddress(networkType).toString();
    // Full kaspa address: kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    console.info(`Full kaspa address: ${kaspaAddress}`);

    console.info(kaspaAddress);
    const keypair = privateKey.toKeypair();
    console.info(keypair.xOnlyPublicKey); // dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(keypair.publicKey);      // 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659

    let rpcUrl = RpcClient.parseUrl("127.0.0.1", encoding, networkType);
    const rpc = new RpcClient(encoding, rpcUrl, networkType);
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    let { isSynced, virtualDaaScore } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }

    let entries = await rpc.getUtxosByAddresses([kaspaAddress]);

    if (!entries.length) {
        console.error("No UTXOs found for address");
    } else {
        console.info(entries);

        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.utxoEntry.amount > b.utxoEntry.amount || -(a.utxoEntry.amount < b.utxoEntry.amount));

        let { transactions, summary } = await createTransactions({
            entries, 
            outputs : [[destinationAddress, kaspaToSompi(0.00012)]],
            priorityFee: 0,
            changeAddress: skAddress,
        });

        console.log("Summary:", summary);

        for (let pending of transactions) {
            console.log("Pending transaction:", pending);
            console.log("Signing tx with secret key:",sk.toString());
            await pending.sign([sk]);
            console.log("Submitting pending tx to RPC ...")
            let txid = await pending.submit(rpc);
            console.log("Node responded with txid:", txid);
        }
    }

    rpc.disconnect();

})();