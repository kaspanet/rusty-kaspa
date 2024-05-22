import {version, Wallet} from "../../../../nodejs/kaspa";

import {w3cwebsocket} from "websocket";
(globalThis.WebSocket as any) = w3cwebsocket;

(async()=>{
    let wallet = new Wallet({resident: false});
    console.log("wallet", wallet)
    let response = await wallet.walletCreate({
        walletSecret: "abc",
        filename: "aaaaaa__xxx3",
        title: "XX2"
    });

    console.log("response", response)
})();