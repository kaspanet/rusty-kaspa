// Run with: node demo.js
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    Address,
    RpcClient,
    Encoding,
    NetworkType,
    createTransaction,
    signTransaction,
    initConsolePanicHook
} = require('./kaspa/kaspa_wasm');
const {parseArgs} = require("./utils");

initConsolePanicHook();

async function runDemo() {
    const args = parseArgs({});

    // Either NetworkType.Mainnet or NetworkType.Testnet
    const networkType = args.networkType;
    // Either Encoding.Borsh or Encoding.SerdeJson
    const encoding = args.encoding;

    // Create secret key from BIP0340
    const sk = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const keypair = sk.toKeypair();

    // For example dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(keypair.xOnlyPublicKey);
    // For example 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(keypair.publicKey);

    // An address such as kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    const address = keypair.toAddress(networkType);
    console.info(`Full kaspa address: ${address}`);
    console.info(address);

    const rpcHost = "127.0.0.1";
    // Parse the url to automatically determine the port for the given host
    const rpcUrl = RpcClient.parseUrl(rpcHost, encoding, networkType);
    const rpc = new RpcClient(encoding, rpcUrl, networkType);

    await rpc.connect();
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }


    try {
        const utxos = await rpc.getUtxosByAddresses({addresses: [address]});

        console.info(utxos);

        if (utxos.length === 0) {
            console.info('Send some kaspa to', address, 'before proceeding with the demo');
            return;
        }


        let total = utxos.reduce((agg, curr) => {
            return curr.utxoEntry.amount + agg;
        }, 0n);

        console.info('Amount sending', total - BigInt(utxos.length) * 2000n)

        const outputs = [{
            address,
            amount: total - BigInt(utxos.length) * 2000n,
        }];

        const changeAddress = address;
        console.info(changeAddress);
        const tx = createTransaction(utxos, outputs, changeAddress, 0n, 0, 1, 1);

        console.info(tx);

        const transaction = signTransaction(tx, [sk], true);
        console.info(JSON.stringify(transaction, null, 4));

        let result = await rpc.submitTransaction(transaction);

        console.info(result);
    } finally {
        await rpc.disconnect();
    }
}

runDemo();
