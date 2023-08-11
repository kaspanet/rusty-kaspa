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
    init_console_panic_hook
} = require('./kaspa/kaspa_wasm');

init_console_panic_hook();

async function runDemo() {
    // From BIP0340
    const sk = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');

    const kaspaAddress = sk.toKeypair().toAddress(NetworkType.Mainnet).toString();
    // Full kaspa address: kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    console.info(`Full kaspa address: ${kaspaAddress}`);

    const addr = new Address(kaspaAddress);
    console.info(addr);

    console.info(sk.toKeypair().xOnlyPublicKey); // dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(sk.toKeypair().publicKey);      // 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659

    const rpcUrl = "ws://127.0.0.1:17110";
    const rpc = new RpcClient(Encoding.Borsh, rpcUrl);

    await rpc.connect();

    try {
        const utxos = await rpc.getUtxosByAddresses({ addresses: [addr.toString()] });

        console.info(utxos);

        if (utxos.length === 0) {
            console.info('Send some kaspa to', kaspaAddress, 'before proceeding with the demo');
            return;
        }

        
        let total = utxos.reduce((agg, curr) => {
            return curr.utxoEntry.amount + agg;
        }, 0n);

        console.info('Amount sending', total - BigInt(utxos.length) * 2000n)

        const outputs = [{
            address: addr.toString(),
            amount: total - BigInt(utxos.length) * 2000n,
        }];

        const changeAddress = new Address(kaspaAddress);
        console.info(changeAddress);
        const tx = createTransaction(utxos, outputs, changeAddress, 0n, 0, 1, 1);

        console.info(tx)

        let transaction = signTransaction(tx, [sk], true);

        let rpcTransaction = transaction.toRpcTransaction();

        console.info(JSON.stringify(rpcTransaction, null, 4));

        let result = await rpc.submitTransaction({transaction: rpcTransaction, allowOrphan:false});

        console.info(result);
    } finally {
        await rpc.disconnect();
    }
}

runDemo();