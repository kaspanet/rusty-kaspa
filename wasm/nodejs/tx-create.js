globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('./kaspa/kaspa_wasm');
let { RpcClient, UtxoSet, Address, Encoding, UtxoOrdering, 
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
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    
    let URL = "ws://127.0.0.1:17110";
    let rpc = new RpcClient(Encoding.Borsh,URL);
    
    console.log(`# connecting to ${URL}`)
    await rpc.connect();
    
    let info = await rpc.getInfo();
    console.log("info", info);
    
    let addresses = [
        new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd"),
        //new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd")
    ];

    console.log("\ngetting UTXOs...");
    let utxos_by_address = await rpc.getUtxosByAddresses({ addresses });
    //console.log("utxos_by_address", utxos_by_address.entries.slice(0, 2))
    let utxoSet = UtxoSet.from(utxos_by_address);

    let amount = 1000n;
    let utxo_selection = await utxoSet.select(amount+100n, UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxo_selection.amount)
    console.log("utxo_selection.totalAmount", utxo_selection.totalAmount)
    let utxos = utxo_selection.utxos;
    console.log("utxos", utxos)
    console.log("utxos.*.data.outpoint", utxos.map(a=>a.data.outpoint))
    console.log("utxos.*.data.entry", utxos.map(a=>a.data.entry))

    let outputItems = [new PaymentOutput(
        new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd"),
        amount
    )];

    let priorityFee = 0n;
    let change_address = new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd");
    // let change = utxo_selection.totalAmount - amount - priorityFee;
    // if (change > 500){
    //     outputItems.push(new Output(
    //         change_address,
    //         change
    //     ))
    // }

    let outputs = new PaymentOutputs(outputItems)
    
    let utxoEntryList = [];
    let inputs = utxos.map((utxo, sequence)=>{
        utxoEntryList.push(utxo.data);
        
        return new TransactionInput({
            previousOutpoint: utxo.data.outpoint,
            signatureScript:[],
            sequence,
            sigOpCount:0
        });
    });

    let utxoEntries = new UtxoEntries(utxoEntryList);

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

    let minimumFee = minimumTransactionFee(transaction, NetworkType.Testnet);

    console.log("minimumFee:", minimumFee);

    let xkey = new XPrivateKey(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    let private_key = xkey.receiveKey(0);
    
    let signableTx = new MutableTransaction(transaction, utxoEntries);
    let adjustTransactionResult = adjustTransactionForFee(signableTx, change_address, priorityFee);
    console.log("adjustTransactionResult", adjustTransactionResult)
    transaction = signTransaction(signableTx, [private_key], true);
    transaction = transaction.toRpcTransaction();
    let result = await rpc.submitTransaction({transaction, allowOrphan:false});

    console.log("result", result)

    await rpc.disconnect();

})();