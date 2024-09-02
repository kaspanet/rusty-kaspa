// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../../../../nodejs/kaspa');
const { parseArgs } = require("../utils");
const {
    Resolver,
    Encoding,
    RpcClient,
} = kaspa;

kaspa.initConsolePanicHook();

const {
    networkId,
    encoding,
} = parseArgs();

(async () => {

    const resolver = new Resolver();
    const rpc = new RpcClient({
        resolver,
        networkId
    });

    await rpc.connect();
    console.log("Connected to", rpc.url);
    const info = await rpc.getBlockDagInfo();

    let hash = info.virtualParentHashes[0];
    let response = await rpc.getBlock({hash, includeTransactions: true});
    
    console.log(response);

    // let transaction = block.transactions[1].inputs[0].previousOutpoint;
    // console.log(transaction);

    await rpc.disconnect();
})();
