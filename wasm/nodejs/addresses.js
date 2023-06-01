globalThis.WebSocket = require('websocket').w3cwebsocket;

let kaspa = require('./kaspa/kaspa_wasm');
let {
    RpcClient,
    Encoding,
    XPublicKey,
    createAddress,
    NetworkType,
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    
    let xpub = await XPublicKey.fromMasterXPrv(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    console.log("xpub", xpub)

    let keys = await xpub.receivePubkeys(0, 10);
    console.log("receive address  keys", keys);
    let addresses = keys.map(key=>createAddress(key, NetworkType.Mainnet).toString());
    console.log("receive addresses", addresses);

    keys = await xpub.changePubkeys(0, 10);
    console.log("change address keys", keys)
    addresses = keys.map(key=>createAddress(key, NetworkType.Mainnet).toString());
    console.log("change addresses", addresses);

})();