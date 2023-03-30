BigInt.prototype["toJSON1"] = function(){
    return this.toString()
}

// let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa');
let kaspa = require('./kaspa/kaspa_wasm');
kaspa.init_console_panic_hook();

// let txid = new kaspa.Hash("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3");
let txid = "880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3";

let keypair1 = kaspa.generate_random_keypair_not_secure();
//console.log("keypair:",keypair1);
let keypair2 = kaspa.generate_random_keypair_not_secure();
//console.log("keypair2:",keypair2);
let keypair3 = kaspa.generate_random_keypair_not_secure();
//console.log("keypair3:",keypair3);

//let scriptPubKey1Bytes = keypair1.publicKey;// '20'+keypair.xOnlyPublicKey+'ac';
//console.log("scriptPubKeyBytes:",scriptPubKey1Bytes);
//console.log("scriptPubKeyBytes:",scriptPubKey1Bytes);

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

//console.log("inputs:",inputs);

// console.log("scriptPubKey:",scriptPubKeyBytes, typeof scriptPubKey);
let scriptPublicKey1 = new kaspa.ScriptPublicKey(0, keypair1.publicKey);
console.log("scriptPublicKey1:",scriptPublicKey1);
let scriptPublicKey2 = new kaspa.ScriptPublicKey(0, keypair2.publicKey);
console.log("scriptPublicKey2:",scriptPublicKey2);

let utxos = [
    new kaspa.UtxoEntry(18446744073709551615n, scriptPublicKey1, 0n, false),
    new kaspa.UtxoEntry(340282366920938463463374607431768211455n, scriptPublicKey2, 0n, false),
    //new kaspa.UtxoEntry(310n, scriptPublicKey1, 0n, false),
    {
        amount: 310n,
        scriptPublicKey: {
          version: 0,
          script: keypair1.publicKey
        },
        blockDaaScore: 0n,
        isCoinbase: false
    }
];

console.log("utxos", utxos)

let utxoEntries = new kaspa.UtxoEntries(utxos);
//console.log("utxoEntries:", utxoEntries.items);

let outputs = [
    new kaspa.TransactionOutput(300n, new kaspa.ScriptPublicKey(0, keypair3.publicKey)),
    {
        value: 300n,
        scriptPublicKey : new kaspa.ScriptPublicKey(0, keypair3.publicKey)
    },
];

//console.log("outputs:",outputs);

let transaction = new kaspa.Transaction({
    inputs,
    outputs,
    lockTime: 1615462089000,
    subnetworkId: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    version: 1,
    gas: 0,
    payload: [],
});

// transaction.inputs.push(new kaspa.TransactionInput({
//     previousOutpoint: { transactionId: txid, index: 2 },
//     signatureScript: [],
//     sequence: 2,
//     sigOpCount: 0
// }));
// transaction.inputs.push(new kaspa.TransactionInput({
//     previousOutpoint: { transactionId: txid, index: 2 },
//     signatureScript: [],
//     sequence: 2,
//     sigOpCount: 0
// }));

// console.log("transaction.inputs.length", transaction.inputs.length)

let signableTx = new kaspa.MutableTransaction(transaction, utxoEntries);
let json = signableTx.toJSON();
console.log("\nsignableTx.toJSON()", json);
console.log("\nsignableTx.getScriptHashes()", signableTx.getScriptHashes());

signableTx = kaspa.MutableTransaction.fromJSON(json);
// console.log("\nJSON casting test: ", signableTx.toJSON()==json);

// console.log("\njson parse direct", JSON.parse(json));
// console.log("\njson parse via helper", JSON.parse(json, (key, value, c)=>{
//     if (["amount"].includes(key)){
//         return BigInt(value)
//     }
//     console.log("#######", {key, value, c})
//     return value;
// }));

//console.log("signableTx.entries.items", signableTx.entries.items)
let keys = [
    keypair2.privateKey,
    //keypair1.privateKey
]

console.log("\nkeys", keys)

transaction = kaspa.signTransaction(signableTx, keys, false);

console.log("\ntransaction:", transaction);

let signer = new kaspa.Signer([
    keypair1.privateKey
]);

console.log("\nsigner:", signer)

// Option 1
// transaction = signer.signTransaction(transaction, true);

// Option 2
transaction = kaspa.signTransaction(transaction, signer, true);

console.log("\ntransaction:", transaction);





//console.log("transaction:", JSON.stringify(transaction, null, "\t"));
// console.log("transaction (JSON):", JSON.stringify(transaction,(k,v) => {
//     console.log(k,v,typeof v);
//     if (typeof v == 'bigint') {
//         return v.toString();
//     } else {
//         return v;
//     }
// },"\t"));

// TODO sign
