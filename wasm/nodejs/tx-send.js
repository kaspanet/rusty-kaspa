globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

// let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa');
let kaspa = require('./kaspa/kaspa_wasm');
let { RpcClient, UtxoSet, Address, Encoding, UtxoOrdering, 
    Outputs, Output, 
    XPrivateKey,
    VirtualTransaction,
    createTransaction,
    signTransaction,
    Person,
    Address1,
    Location
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

    //let addresses = ["kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd"];
    //console.log("\naddresses:", addresses);
    console.log("\nJSON.stringify(addresses):", JSON.stringify(addresses));
    //console.log("\naddresses.toString():", addresses.toString());
    // console.log(addresses.toString());

    console.log("\ngetting UTXOs...");
    let utxos_by_address = await rpc.getUtxosByAddresses({ addresses });
    console.log("Creating UtxoSet...");
    //console.log("utxos_by_address", utxos_by_address)
    let utxoSet = UtxoSet.from(utxos_by_address);

    //console.log("utxos_by_address", utxos_by_address)

    let amount = 1000n;

    let utxo_selection = await utxoSet.select(amount+100n, UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxo_selection.amount)
    console.log("utxo_selection.totalAmount", utxo_selection.totalAmount)

    let output = new Output(
        new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd"),
        amount
    );
    //console.log("output", output)
    let outputs = new Outputs([output])

    console.log("outputs", outputs)

    let change_address = new Address("kaspatest:qz7ulu4c25dh7fzec9zjyrmlhnkzrg4wmf89q7gzr3gfrsj3uz6xjceef60sd");

    let priorityFee = 0;
    let tx = createTransaction(utxo_selection, outputs, change_address, priorityFee);
    console.log("tx", tx)

    let xkey = new XPrivateKey(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    let private_key = xkey.receiveKey(0);

    console.log("tx.inputs", tx.inputs)

    let transaction = signTransaction(tx, [private_key], true);
    transaction = transaction.toRpcTransaction();
    let result = await rpc.submitTransaction({transaction, allowOrphan:false});

    console.log("result", result)

    await rpc.disconnect();

})();