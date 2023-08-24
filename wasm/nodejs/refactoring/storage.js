globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../kaspa/kaspa_wasm');
const {parseArgs} = require("../utils");
kaspa.init_console_panic_hook();

(async () => {
    const {networkType} = parseArgs();

    const wallet = new kaspa.Wallet({
        resident: true,
        networkType: networkType,
    });

    wallet.events.setHandler((args) => {
        console.log("multiplexer event: ", args);
    })

    const walletSecret = "secret";

    const descriptor = await wallet.createWallet({
        walletSecret,
    });

    console.log("descriptor:", descriptor);

    const keyData = await wallet.createPrvKeyData({
        mnemonic: "fade insect feature mobile impose dinosaur brisk congress soul civil spoil cute maximum resemble zoo tower joke era luxury file eager business empower giggle",
        walletSecret
    });

    console.log("keydata:", keyData);

    const account = await wallet.createAccount(keyData.id, {
        accountKind: kaspa.AccountKind.Bip32,
        walletSecret
    });

    console.log("account:", account);

    // await wallet.start();

    // try {

    //     // await wallet.connect();
    //     await wallet.connect({
    //         // url : "ws://foobar",
    //         url : "wrpc://127.0.0.1:17110",
    //         retry : false,
    //         block : true,
    //     });
    //     // .catch((e) => {
    //     //     console.log("connect error:",e);
    //     // });
    //     console.log("wallet:",wallet);

    //     let info = await wallet.rpc.getBlockDagInfo();
    //     console.log("info:", info);

    //     await wallet.disconnect();
    //     await wallet.stop();
    // } catch(e) {
    //     console.log("Client-side error:",e);
    //     await wallet.disconnect();
    //     await wallet.stop();
    // }

})();

// WIP
class Storage {
    async exists(name) {
    }

    async create(ctx, args) {
    }

    async open(ctx, args) {
    }

    async commit(ctx) {
    }

    async close(ctx) {
    }

    // isOpen() {}
    // descriptor() {}

    async getKeyInfoRange(start, stop) {
    }

    async loadKeyInfo(ids) {
    }

    async loadKeyData(id) {
    }

    async storeKeyInfo(data) {
    }

    async storeKeyData(data) {
    }

    async removeKeyData(ids) {
    }

    async getAccountRange(start, stop) {
    }

    async getAccountCount(id) {
    }

    async loadAccounts(ids) {
    }

    async storeAccounts(accounts) {
    }

    async removeAccounts(ids) {
    }

    async getTransactonRecordRange(start, stop) {
    }

    async loadTransactionRecords(ids) {
    }

    async storeTransactionRecords(transaction_records) {
    }

    async removeTransactionRecords(ids) {
    }

}
