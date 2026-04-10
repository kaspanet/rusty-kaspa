// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket; // W3C WebSocket module shim

const kaspa = require("../../../../nodejs/kaspa");
const { RpcClient, Encoding } = kaspa;

kaspa.initConsolePanicHook();

(async () => {
  const rpc = new RpcClient({
    url: "wss://devnet2-wrpc.kasia.fyi",
    encoding: Encoding.Borsh,
    // resolver: new Resolver(),
    networkId: "mainnet",
  });
  console.log(`Resolving RPC endpoint...`);
  await rpc.connect();
  console.log(`Connecting to ${rpc.url}`);

  const response = await rpc.getMempoolEntries({
    includeOrphanPool: true,
    filterTransactionPool: false,
  });
  console.log(`${response.mempoolEntries.length} transations in the mempool`);
})();
