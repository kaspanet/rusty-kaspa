globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../kaspa/kaspa_wasm');
const {parseArgs, guardRpcIsSynced} = require("../utils");
const {
    RpcClient, UtxoSet, Address, Encoding, UtxoOrdering,
    PaymentOutputs, PaymentOutput,
    XPrivateKey,
    VirtualTransaction,
    createTransaction,
    signTransaction,
    Person,
    Address1,
    Location
} = kaspa;
kaspa.init_console_panic_hook();

(async () => {
    const {
        encoding,
        address,
    } = parseArgs();

    const URL = "ws://127.0.0.1:17110";
    const rpc = new RpcClient(encoding, URL);

    console.log(`# connecting to ${URL}`)
    await rpc.connect();
    await guardRpcIsSynced(rpc);

    const info = await rpc.getInfo();
    console.log("info", info);

    const addr = new Address(address ?? "kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd");
    const addresses = [
        addr,
        //new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")
    ];

    //let addresses = ["kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd"];
    //console.log("\naddresses:", addresses);
    console.log("\nJSON.stringify(addresses):", JSON.stringify(addresses));
    //console.log("\naddresses.toString():", addresses.toString());
    // console.log(addresses.toString());

    console.log("\ngetting UTXOs...");
    const utxosByAddress = await rpc.getUtxosByAddresses({addresses});
    console.log("Creating UtxoSet...");
    //console.log("utxos_by_address", utxos_by_address)
    const utxoSet = UtxoSet.from(utxosByAddress);

    //console.log("utxos_by_address", utxos_by_address)

    const amount = 1000n;

    const utxoSelection = await utxoSet.select(amount + 100n, UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxoSelection.amount)
    console.log("utxo_selection.totalAmount", utxoSelection.totalAmount)
    const utxos = utxoSelection.utxos;
    console.log("utxos", utxos)
    console.log("utxos.*.data.entry", utxos.map(a => a.data.entry))


    const output = new PaymentOutput(
        addr,
        amount
    );
    //console.log("output", output)
    const outputs = new PaymentOutputs([output])

    console.log("outputs", outputs)

    const changeAddress = addr;

    const priorityFee = 0;
    const tx = createTransaction(utxoSelection, outputs, changeAddress, priorityFee);
    console.log("tx", tx)

    const xKey = new XPrivateKey(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    const private_key = xKey.receiveKey(0);

    console.log("tx.inputs", tx.inputs)

    let transaction = signTransaction(tx, [private_key], true);
    transaction = transaction.toRpcTransaction();
    const result = await rpc.submitTransaction({transaction, allowOrphan: false});

    console.log("result", result)

    await rpc.disconnect();
})();
