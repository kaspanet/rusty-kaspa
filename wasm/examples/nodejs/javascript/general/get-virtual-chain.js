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
      "493435989075383ba92669d2bdcf12b6e894dac7d42e767aa93994637e8e5cd0",
    includeAcceptedTransactionIds: true,
    minConfirmationCount: 1,
  });
  console.log(
    "GetVirtualChainFromBlock response:",
    virtualChainFromBlockResponse
  );

  await rpc.disconnect();
  console.log("bye!");
})();
