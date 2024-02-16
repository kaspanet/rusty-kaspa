// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const path = require('path');
const fs = require('fs');
const kaspa = require('../../../../nodejs/kaspa');
const {
    Wallet, setDefaultStorageFolder
} = kaspa;

let storageFolder = path.join(__dirname, '../../../data/wallets').normalize();
if (!fs.existsSync(storageFolder)) {
    fs.mkdirSync(storageFolder);
}
setDefaultStorageFolder(storageFolder);

(async()=>{
    try {

        let wallet = new Wallet({resident: false});
        console.log("wallet", wallet)
        let response = await wallet.walletCreate({
            walletSecret: "abc",
            filename: "aaaaaa__xxx3",
            title: "XX2"
        });

        console.log("response", response)
    } catch(ex) {
        console.error("Error:", ex);
    }
})();