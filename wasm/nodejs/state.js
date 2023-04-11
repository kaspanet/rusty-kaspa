let kaspa = require('./kaspa/kaspa_wasm');
kaspa.init_console_panic_hook();

const {
    Header, State, Hash
} = kaspa;

(async ()=>{
    let header = new Header(
        0,//version
        [[new Hash("0000000000000000000000000000000000000000000000000000000000000000")]],//parents_by_level_array
        "0000000000000000000000000000000000000000000000000000000000000000",//hash_merkle_root
        "0000000000000000000000000000000000000000000000000000000000000000",//accepted_id_merkle_root
        "0000000000000000000000000000000000000000000000000000000000000000",//utxo_commitment
        0n,//timestamp
        0,//bits
        0n,//nonce
        0n,//daa_score
        0n,//blue_work
        0n,//blue_score
        "0000000000000000000000000000000000000000000000000000000000000000"//pruning_point
    );

    let header_hash = header.calculateHash();
    console.log("header_hash", header_hash, header_hash+"");

    let state = new State(header);

    let [a, v] = state.checkPow(0n);

    console.log("state", a, v, v.toBigInt())

})();

