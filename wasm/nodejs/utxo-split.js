globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('./kaspa/kaspa_wasm');
let { 
    RpcClient,
    UtxoSet,
    Address,
    Encoding,
    UtxoOrdering, 
    PaymentOutputs,
    PaymentOutput, 
    XPrivateKey,
    VirtualTransaction,
    createTransaction,
    signTransaction,
    calculateTransactionMass,
    Person,
    Location,
    NetworkType,
    UtxoEntries,
    LimitCalcStrategy,
    Abortable
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
        new Address("kaspa:qq5dawejjdzp22jsgtn2mdr3lg45j7pq0yaq8he8t8269lvg87cuwl7ze7djh"),
        new Address("kaspa:qzewpzt0rx6jmvy0eea82lpnf0t7f7frmqavqcaawmt4wk70puazcp8zljgx5"),
    ];

    console.log("\ngetting UTXOs...");
    let utxos_by_address = await rpc.getUtxosByAddresses({ addresses });
    console.log("\nCreating UtxoSet...");
    let utxoSet = UtxoSet.from(utxos_by_address);

    //console.log("utxos_by_address", utxos_by_address)
    let count = 90;

    let amount = 10000n;

    let utxo_selection = await utxoSet.select(amount * BigInt(count), UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxo_selection.amount)
    console.log("utxo_selection.totalAmount", utxo_selection.totalAmount)
    //let utxos = utxo_selection.utxos;

    let outputs = [];
    for (let i=0; i<count; i++){
        let output = new PaymentOutput(
            new Address("kaspa:qq5dawejjdzp22jsgtn2mdr3lg45j7pq0yaq8he8t8269lvg87cuwl7ze7djh"),
            amount
        );
        outputs.push(output)
    }
    let priorityFee = 0n;
    outputs = new PaymentOutputs(outputs)

    //console.log("outputs", outputs)

    let xkey = new XPrivateKey(
        "kprv...",
        false,
        0n
    );

    let private_keys = [];
    private_keys.push(xkey.changeKey(0));
    private_keys.push(xkey.receiveKey(0));

    let change_address = new Address("kaspa:qq5dawejjdzp22jsgtn2mdr3lg45j7pq0yaq8he8t8269lvg87cuwl7ze7djh");
    let result = [];
    
    if (false){
        let tx = createTransaction(1, utxo_selection, outputs, change_address, 1, priorityFee);
        //console.log("tx", tx)
        let transaction = signTransaction(tx, private_keys, true);
        result = [await rpc.submitTransaction({transaction, allowOrphan:false})];
        // console.log("result", result)
    }else{
        let vt = await new VirtualTransaction(
            1,
            1,
            utxo_selection,
            outputs,
            change_address,
            priorityFee,
            [],
            LimitCalcStrategy.calculated(),
            new Abortable()
        );
        vt.sign(private_keys, true);
        //txs = vt.transactions();
        //result = await vt.submit(rpc, false);
    }

    // console.log("txs.length", txs.length)
    // for(transaction of txs){
    //     console.log("inputs length", transaction.inputs.length)
    //     let mass = transaction.mass(NetworkType.Mainnet, false, 1);
    //     console.log("mass after sign", mass);
    //     transaction = transaction.toRpcTransaction();
    //     // let result = await rpc.submitTransaction({transaction, allowOrphan:false});
    //     // console.log("result", result)
    // }

    console.log("result", result)

    await rpc.disconnect();

})();