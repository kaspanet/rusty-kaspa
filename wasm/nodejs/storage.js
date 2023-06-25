globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

let kaspa = require('./kaspa/kaspa_wasm');
kaspa.init_console_panic_hook();

(async ()=>{
    
    let wallet = new kaspa.Wallet();
    wallet.events.setHandler((args) => {
        console.log("multiplexer event: ",args);
    })
    await wallet.start();

    try {

        // await wallet.connect();
        await wallet.connect({
            // url : "ws://foobar",
            url : "wrpc://127.0.0.1:17110",
            retry : false,
            block : true,
        });
        // .catch((e) => {
        //     console.log("connect error:",e);
        // });
        console.log("wallet:",wallet);

        let info = await wallet.rpc.getBlockDagInfo();
        console.log("info:", info);

        await wallet.disconnect();
    } catch(e) {
        console.log("Client-side error:",e);
        wallet.disconnect();
    }

})();

// WIP
class Storage {
    async exists(name) { }
    async create(ctx,args) { }
    async open(ctx,args) { }
    async commit(ctx) {}
    async close(ctx) {}
    // isOpen() {}
    // descriptor() {}

    async getKeyInfoRange(start, stop) {}
    async loadKeyInfo(ids) {}
    async loadKeyData(id) {}
    async storeKeyInfo(data) {}
    async storeKeyData(data) {}
    async removeKeyData(ids) {}

    async getAccountRange(start, stop) {}
    async getAccountCount(id) {}
    async loadAccounts(ids) {}
    async storeAccounts(accounts) {}
    async removeAccounts(ids) {}

    async getTransactonRecordRange(start, stop) {}
    async loadTransactionRecords(ids) {}
    async storeTransactionRecords(transaction_records) {}
    async removeTransactionRecords(ids) {}

}