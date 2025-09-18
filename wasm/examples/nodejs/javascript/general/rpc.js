// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../../../../nodejs/kaspa');
const { parseArgs } = require("../utils");
const {
    RpcClient,
    Resolver,
} = kaspa;

kaspa.initConsolePanicHook();

const {
    networkId,
    encoding,
} = parseArgs();

(async () => {

    const rpc = new RpcClient({
        // url : "127.0.0.1",
        // encoding,
        resolver: new Resolver(),
        networkId
    });
    console.log(`Resolving RPC endpoint...`);
    await rpc.connect();
    console.log(`Connecting to ${rpc.url}`)

    const info = await rpc.getBlockDagInfo();
    console.log("GetBlockDagInfo response:", info);

    // const address = await rpc.getUtxoReturnAddress({txid: "a1f9a403e3c82e9b6dc7436682878262133ff0dd3fbc2d63c5f1973f79fa2b4e", acceptingBlockDaaScore: 165553103n})
    // console.log("getUtxoReturnAddress response:", address);

    await rpc.disconnect();
    console.log("bye!");
})();
