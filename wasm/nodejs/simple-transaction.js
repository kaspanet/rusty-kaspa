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
    init_console_panic_hook
} = require('./kaspa/kaspa_wasm');
const {parseArgs, guardRpcIsSynced} = require('./utils');

init_console_panic_hook();

async function runDemo() {
    let {
        address: destinationAddress,
        networkType,
        encoding,
    } = parseArgs();
    destinationAddress = destinationAddress ?? "kaspatest:qqkl0ct62rv6dz74pff2kx5sfyasl4z28uekevau23g877r5gt6userwyrmtt";
    console.log("using destination address:", destinationAddress);

    // From BIP0340
    const sk = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');

    const kaspaAddress = sk.toKeypair().toAddress(networkType).toString();
    // Full kaspa address: kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    console.info(`Full kaspa address: ${kaspaAddress}`);

    const skAddress = new Address(kaspaAddress);
    console.info(skAddress);
    console.info(sk.toKeypair().xOnlyPublicKey); // dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(sk.toKeypair().publicKey);      // 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659

    let rpcUrl = RpcClient.parseUrl("127.0.0.1", encoding, networkType);
    const rpc = new RpcClient(encoding, rpcUrl, networkType);
    console.log(`Connecting to ${rpc.url}`);

    await rpc.connect();
    await guardRpcIsSynced(rpc);

    let entries = await rpc.getUtxosByAddresses([skAddress]);

    if (!entries.length) {
        console.error("No UTXOs found for address");
    } else {
        console.info(entries);

        // a very basic JS-driven utxo entry sort
        entries.sort((a, b) => a.utxoEntry.amount > b.utxoEntry.amount || -(a.utxoEntry.amount < b.utxoEntry.amount));

        let total = entries.reduce((agg, curr) => {
            return curr.utxoEntry.amount + agg;
        }, 0n);

        console.info('Amount sending', total - BigInt(entries.length) * 2000n)

        let { transactions, summary } = await createTransactions({
            entries, 
            outputs : [[destinationAddress, total - BigInt(entries.length) * 2000n]],
            priorityFee: 0,
            changeAddress: destinationAddress,
        });

        console.log("summary:", summary);

        for (let pending of transactions) {
            console.log("pending transaction:", pending);
            console.log("signing tx with secret key:",sk.toString());
            await pending.sign([sk]);
            console.log("submitting pending tx to RPC ...")
            let txid = await pending.submit(rpc);
            console.log("node responded with txid:", txid);
        }
    }

    rpc.disconnect();
}

runDemo();
