globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

// let {RpcClient,Encoding,init_console_panic_hook,defer} = require('./kaspa');
let kaspa = require('./kaspa/kaspa_wasm');
let { RpcClient, Encoding,
    XPublicKey,
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    
    // let URL = "ws://127.0.0.1:17110";
    // let rpc = new RpcClient(Encoding.Borsh,URL);
    
    // console.log(`# connecting to ${URL}`)
    // await rpc.connect();
    
    // let info1 = await rpc.getInfo();
    // console.log(info1);

    let xpub = await XPublicKey.fromXPrv(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    console.log("xpub", xpub)

    let addresses = await xpub.receiveAddresses(10, 0);
    console.log("receive addresses", addresses)
    addresses = await xpub.changeAddresses(10, 0);
    console.log("change addresses", addresses)

    //await rpc.disconnect();

})();