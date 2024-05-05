globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('../kaspa/kaspa_wasm');
const { parseArgs, guardRpcIsSynced } = require("../utils");
let {
    RpcClient, UtxoSet, Address, Encoding, UtxoOrdering,
    PaymentOutputs, PaymentOutput,
    XPrivateKey,
    VirtualTransaction,
    createTransaction,
    signTransaction,
    signScriptHash
} = kaspa;
kaspa.init_console_panic_hook();

(async () => {
    const args = parseArgs({});

    // Either NetworkType.Mainnet or NetworkType.Testnet
    const networkType = args.networkType;
    // Either Encoding.Borsh or Encoding.JSON
    const encoding = args.encoding;
    // The kaspa address that was passed as an argument or a default one
    const address = args.address ?? "kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd";

    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });

    console.log(`# connecting to ${URL}`)
    await rpc.connect();
    console.log(`# connected ...`)
    await guardRpcIsSynced(rpc);

    const info1 = await rpc.getInfo();
    console.log(info1);
    const info2 = await rpc.getInfo();
    console.log(info2);

    const addresses = [address];

    console.log("\nJSON.stringify(addresses):", JSON.stringify(addresses));

    console.log("\ngetting UTXOs...");
    const utxosByAddress = await rpc.getUtxosByAddresses({ addresses });
    console.log("Creating UtxoSet...");
    //console.log("utxos_by_address", utxos_by_address)
    const utxoSet = UtxoSet.from(utxosByAddress);

    //console.log("utxos_by_address", utxos_by_address)

    const amount = 1000n;

    const utxoSelection = await utxoSet.select(amount + 100n, UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxoSelection.amount)
    console.log("utxo_selection.totalAmount", utxoSelection.totalAmount)

    const outputs = [
        [
            address,
            amount
        ]
    ];

    console.log("outputs", outputs)

    const changeAddress = addr;

    const priorityFee = 1500;
    const tx = createTransaction(utxoSelection, outputs, changeAddress, priorityFee);
    const scriptHashes = tx.getScriptHashes();
    console.log("scriptHashes", scriptHashes)

    const xKey = new XPrivateKey(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    const private_key = xKey.receiveKey(0);

    const signatures = scriptHashes.map(hash => signScriptHash(hash, private_key));
    console.log("signatures", signatures)

    const transaction = tx.setSignatures(signatures);
    console.log("transaction", transaction)
    //let transaction = tx.toRpcTransaction();

    const result = await rpc.submitTransaction(transaction);

    console.log("result", result)

    await rpc.disconnect();
})();
