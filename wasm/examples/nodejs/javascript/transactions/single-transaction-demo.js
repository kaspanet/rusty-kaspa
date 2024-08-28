// Run with: node demo.js
// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket;

const {
    PrivateKey,
    RpcClient,
    createTransaction,
    signTransaction,
    initConsolePanicHook,
    Resolver,
} = require('../../../../nodejs/kaspa');

initConsolePanicHook();

// command line arguments --network=(mainnet|testnet-<number>) --encoding=borsh (default)
const { networkId, encoding } = require("../utils").parseArgs();

(async () => {

    // Create secret key from BIP0340
    const privateKey = new PrivateKey('b99d75736a0fd0ae2da658959813d680474f5a740a9c970a7da867141596178f');
    const keypair = privateKey.toKeypair();

    // For example dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(keypair.xOnlyPublicKey);
    // For example 02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659
    console.info(keypair.publicKey);

    // An address such as kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva
    const address = keypair.toAddress(networkId);
    console.info(`Full kaspa address: ${address}`);
    console.info(address);

    const rpc = new RpcClient({
        //url : "127.0.0.1",
        resolver: new Resolver(),
        encoding,
        networkId
    });
    console.log(`Connecting to ${rpc.url}`);
    await rpc.connect();
    console.log(`Connected to ${rpc.url}`);
    let { isSynced } = await rpc.getServerInfo();
    if (!isSynced) {
        console.error("Please wait for the node to sync");
        rpc.disconnect();
        return;
    }


    try {
        const { entries : utxos } = await rpc.getUtxosByAddresses([address]);

        console.info(utxos);

        if (utxos.length === 0) {
            console.info('Send some kaspa to', address, 'before proceeding with the demo');
            return;
        }


        let total = utxos.reduce((agg, curr) => {
            return curr.amount + agg;
        }, 0n);

        console.info('Amount sending', total - BigInt(utxos.length) * 2000n)

        const outputs = [{
            address,
            amount: total - BigInt(utxos.length) * 2000n,
        }];

        const changeAddress = address;
        console.log("changeAddress:", changeAddress)
        
        // utxo_entry_source: IUtxoEntry[], 
        // outputs: IPaymentOutput[], 
        // priority_fee: bigint, 
        // payload: HexString | Uint8Array, 
        // sig_op_count?: number
        const tx = createTransaction(utxos, outputs, 0n, "", 1);


        console.info("Transaction before signing:", tx);

        const transaction = signTransaction(tx, [privateKey], true);

        console.log("Transaction:", transaction);
        // console.info(JSON.stringify(transaction, null, 4));

        let result = await rpc.submitTransaction({transaction});

        console.info("submitTransaction result:", result);
    } finally {
        await rpc.disconnect();
    }
})();
