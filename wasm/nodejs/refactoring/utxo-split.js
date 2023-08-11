globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../kaspa/kaspa_wasm');
const {parseArgs, guardRpcIsSynced} = require("../utils");
const {
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

    const addr1 = new Address(address ?? "kaspa:qq5dawejjdzp22jsgtn2mdr3lg45j7pq0yaq8he8t8269lvg87cuwl7ze7djh");
    const addr2 = new Address("kaspa:qzewpzt0rx6jmvy0eea82lpnf0t7f7frmqavqcaawmt4wk70puazcp8zljgx5");
    const addresses = [
        addr1,
        addr2,
    ];

    console.log("\ngetting UTXOs...");
    const utxosByAddress = await rpc.getUtxosByAddresses({addresses});
    console.log("\nCreating UtxoSet...");
    const utxoSet = UtxoSet.from(utxosByAddress);

    //console.log("utxos_by_address", utxos_by_address)
    const count = 90;

    const amount = 10000n;

    const utxo_selection = await utxoSet.select(amount * BigInt(count), UtxoOrdering.AscendingAmount);

    console.log("utxo_selection.amount", utxo_selection.amount)
    console.log("utxo_selection.totalAmount", utxo_selection.totalAmount)
    //const utxos = utxo_selection.utxos;

    let outputs = [];
    for (let i = 0; i < count; i++) {
        const output = new PaymentOutput(
            addr1,
            amount
        );
        outputs.push(output)
    }
    const priorityFee = 0n;
    outputs = new PaymentOutputs(outputs)

    //console.log("outputs", outputs)

    const xKey = new XPrivateKey(
        "kprv...",
        false,
        0n
    );

    const private_keys = [];
    private_keys.push(xKey.changeKey(0));
    private_keys.push(xKey.receiveKey(0));

    const change_address = new Address("kaspa:qq5dawejjdzp22jsgtn2mdr3lg45j7pq0yaq8he8t8269lvg87cuwl7ze7djh");
    let result = [];

    if (false) {
        const tx = createTransaction(1, utxo_selection, outputs, change_address, 1, priorityFee);
        //console.log("tx", tx)
        const transaction = signTransaction(tx, private_keys, true);
        result = [await rpc.submitTransaction({transaction, allowOrphan: false})];
        // console.log("result", result)
    } else {
        const vt = await new VirtualTransaction(
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
    //     const mass = transaction.mass(NetworkType.Mainnet, false, 1);
    //     console.log("mass after sign", mass);
    //     transaction = transaction.toRpcTransaction();
    //     // const result = await rpc.submitTransaction({transaction, allowOrphan:false});
    //     // console.log("result", result)
    // }

    console.log("result", result)

    await rpc.disconnect();
})();
