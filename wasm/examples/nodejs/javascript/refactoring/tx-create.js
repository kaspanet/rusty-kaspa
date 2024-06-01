globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../kaspa/kaspa_wasm');
const { parseArgs, guardRpcIsSynced } = require("../utils");
const {
    RpcClient, UtxoSet, Address, Encoding, UtxoOrdering,
    PaymentOutputs, PaymentOutput,
    XPrivateKey,
    TransactionInput,
    Transaction,
    signTransaction,
    MutableTransaction,
    UtxoEntries,
    NetworkType,
    minimumTransactionFee,
    adjustTransactionForFee,
    Sequence,
} = kaspa;
kaspa.init_console_panic_hook();

(async () => {
    const {
        encoding,
        address,
        networkType,
    } = parseArgs();

    const rpc = new RpcClient({
        url : "127.0.0.1",
        encoding,
        networkId
    });

    console.log(`Connecting to ${rpc.url}`)
    await rpc.connect();
    await guardRpcIsSynced(rpc);

    // let res = await rpc.getBlockTemplate({
    //     extraData:[],
    //     payAddress:"kaspa:qrwee7xc2qw5whq8qzv82qjld6zunwy46lsy3hueej5kvgfwvamhswy03lsyh"
    // });
    // console.log("res", res.block.header.blueWork);

    // return

    const info = await rpc.getInfo();
    console.log("info", info);

    const addr = address ?? "kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd";

    const addresses = [
        addr,
        //new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")
    ];

    console.log("\ngetting UTXOs...", addresses);
    // const utxosByAddress = await rpc.getUtxosByAddresses({addresses});

    const utxos = await rpc.getUtxosByAddresses({ addresses });

    const amount = 1000n;
    // const utxoSelection = await utxoSet.select(amount + 100n, UtxoOrdering.AscendingAmount);
    //
    // console.log("utxo_selection.amount", utxoSelection.amount)
    // console.log("utxo_selection.totalAmount", utxoSelection.totalAmount)
    // // const utxos = utxoSelection.utxos;
    // console.log("utxos[0].data.outpoint", utxos[0]?.data.outpoint)
    // console.log("utxos.*.data.outpoint", utxos.map(a => a.data.outpoint))
    // console.log("utxos.*.data.entry", utxos.map(a => a.data.entry))

    const priorityFee = 0n;
    const changeAddress = addr;
    // let change = utxo_selection.totalAmount - amount - priorityFee;
    // if (change > 500){
    //     outputItems.push(new Output(
    //         change_address,
    //         change
    //     ))
    // }

    const outputs = [
        [
            addr,
            amount
        ]
    ];

    const utxoEntryList = [];
    const inputs = utxos.map((utxo, sequence) => {
        utxoEntryList.push(utxo.data);

        return new TransactionInput({
            previousOutpoint: utxo.data.outpoint,
            signatureScript: [],
            sequence,
            sigOpCount: 0
        });
    });

    const utxoEntries = new UtxoEntries(utxoEntryList);

    console.log("inputs", inputs);
    console.log("outputs", outputs);
    console.log("utxoEntries:", utxoEntries.items);

    // let outputs = [
    //     new kaspa.TransactionOutput(300n, new kaspa.ScriptPublicKey(0, keypair3.publicKey)),
    //     {
    //         value: 300n,
    //         scriptPublicKey : new kaspa.ScriptPublicKey(0, keypair3.publicKey)
    //     },
    // ];

    let transaction = new Transaction({
        inputs,
        outputs,
        lockTime: 0,
        subnetworkId: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        version: 0,
        gas: 0,
        payload: [],
    });

    console.log("transaction", transaction)

    const minimumFee = minimumTransactionFee(transaction, networkType);

    console.log("minimumFee:", minimumFee);

    const xKey = new XPrivateKey(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    const private_key = xKey.receiveKey(0);

    let mtx = new MutableTransaction(transaction, utxoEntries);
    const adjustTransactionResult = adjustTransactionForFee(mtx, changeAddress, priorityFee);
    console.log("adjustTransactionResult", adjustTransactionResult)
    mtx = signTransaction(mtx, [private_key], true);
    console.log("before submit mtx.id", mtx.id)
    transaction = mtx.toRpcTransaction();

    let result = await rpc.submitTransaction({ transaction, allowOrphan: false });

    console.log("result", result)

    await rpc.disconnect();
})();
