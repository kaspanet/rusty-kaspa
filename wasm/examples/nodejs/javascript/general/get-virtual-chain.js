// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket; // W3C WebSocket module shim

const kaspa = require("../../../../nodejs/kaspa");
const { parseArgs } = require("../utils");
const { RpcClient, Resolver } = kaspa;

kaspa.initConsolePanicHook();

const { networkId, encoding } = parseArgs();

(async () => {
  const rpc = new RpcClient({
    // url : "127.0.0.1",
    // encoding,
    resolver: new Resolver(),
    networkId: "mainnet",
  });
  console.log(`Resolving RPC endpoint...`);
  await rpc.connect();
  console.log(`Connecting to ${rpc.url}`);

  const virtualChainFromBlockResponse = await rpc.getVirtualChainFromBlock({
    startHash:
      "106145ef74693458e1013819f04cafb65d99fd17d28a8c0a6e3941199c0adf82",
    includeAcceptedTransactionIds: true,
    // minConfirmationCount: 1,
  });
  console.log(
    "GetVirtualChainFromBlock response:",
    virtualChainFromBlockResponse
  );

  await rpc.disconnect();
  console.log("bye!");
})();
