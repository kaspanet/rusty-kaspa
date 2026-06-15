// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket; // W3C WebSocket module shim

const kaspa = require("../../../../nodejs/kaspa");
const { RpcClient, Encoding, Resolver } = kaspa;

kaspa.initConsolePanicHook();

const BLOCK_HASH =
  "ab36f709e83bf6ba66e9516cbf02b9a9848a1b913e7c59cd5010e90928a27e22";

(async () => {
  const rpc = new RpcClient({
    url: undefined,
    encoding: Encoding.Borsh,
    resolver: new Resolver(),
    networkId: "mainnet",
  });

  console.log("Resolving RPC endpoint...");
  await rpc.connect();
  console.log(`Connected to ${rpc.url}`);

  try {
    console.log("Querying block reward for block:", BLOCK_HASH);

    const rewardInfo = await rpc.getBlockRewardInfo({
      hash: BLOCK_HASH,
    });

    console.log({ rewardInfo });
  } finally {
    await rpc.disconnect();
    console.log("bye!");
  }
})().catch((err) => {
  console.error("GetBlockRewardInfo example failed:", err);
  process.exitCode = 1;
});
