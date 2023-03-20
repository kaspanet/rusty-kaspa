// let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa');
let kaspa = require('./kaspa');
kaspa.init_console_panic_hook();

// let txid = new kaspa.Hash("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3");
let txid = "880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3";

let keypair = kaspa.generate_random_keypair_not_secure();
console.log("keypair:",keypair);

let scriptPubKeyBytes = '20'+keypair.xOnlyPublicKey+'ac';
console.log("scriptPubKeyBytes:",scriptPubKeyBytes);

let inputs =  [
    new kaspa.TransactionInput({
        previousOutpoint: { transactionId: txid, index: 0 },
        signatureScript: [],
        sequence: 0,
        sigOpCount: 0
    }),
    new kaspa.TransactionInput({
        previousOutpoint: { transactionId: txid, index: 1 },
        signatureScript: [],
        sequence: 1,
        sigOpCount: 0
    }),
    new kaspa.TransactionInput({
        previousOutpoint: { transactionId: txid, index: 2 },
        signatureScript: [],
        sequence: 2,
        sigOpCount: 0
    }),
];

console.log("inputs:",inputs);

// console.log("scriptPubKey:",scriptPubKeyBytes, typeof scriptPubKey);
let scriptPublicKey = new kaspa.ScriptPublicKey(0, scriptPubKeyBytes);
console.log("scriptPublicKey:",scriptPublicKey);

let outputs = [
    new kaspa.TransactionOutput(300n, new kaspa.ScriptPublicKey(0, scriptPubKeyBytes)),
    {
        value: 300n,
        scriptPublicKey : new kaspa.ScriptPublicKey(0, scriptPubKeyBytes)
    },
];

console.log("outputs:",outputs);

let transaction = new kaspa.Transaction({
    inputs,
    outputs,
    lockTime: 1615462089000,
    subnetworkId: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    version: 1,
    gas: 0,
    payload: [],
});

console.log("transaction:",transaction);
console.log("transaction (JSON):", JSON.stringify(transaction,(k,v) => {
    console.log(k,v,typeof v);
    if (typeof v == 'bigint') {
        return v.toString();
    } else {
        return v;
    }
},"\t"));

// TODO sign
