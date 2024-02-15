// @ts-ignore
globalThis.WebSocket = require('websocket').w3cwebsocket; // W3C WebSocket module shim

const kaspa = require('../../../../nodejs/kaspa');
const { parseArgs } = require("../utils");
const {
    Beacon,
    Encoding,
    RpcClient,
} = kaspa;

kaspa.initConsolePanicHook();

const {
    networkId,
    encoding,
} = parseArgs();

(async () => {

    const beacon = new Beacon();

    // let url = await beacon.getUrl(Encoding.Borsh, networkId);
    // console.log(url);
    // const rpc = new RpcClient({
    //     url,
    //     encoding,
    //     networkId
    // });

    const rpc = await beacon.connect(networkId);
    console.log("Connected to", rpc.url);

    // console.log(`Connecting to ${rpc.url}`)
    // await rpc.connect();

    const info = await rpc.getBlockDagInfo();
    console.log("GetBlockDagInfo response:", info);

    await rpc.disconnect();
})();
