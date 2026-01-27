// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket; // W3C WebSocket module shim

const kaspa = require("../../../../nodejs/kaspa");
const { RpcClient, Encoding } = kaspa;

kaspa.initConsolePanicHook();

const delay = (ms) => new Promise((res) => setTimeout(res, ms));

(async () => {
  const rpc = new RpcClient({
    url: "127.0.0.1",
    encoding: Encoding.Borsh,
    // resolver: new Resolver(),
    networkId: "mainnet",
  });
  console.log(`Resolving RPC endpoint...`);
  await rpc.connect();
  console.log(`Connecting to ${rpc.url}`);

  console.log("Getting known block hash from node...");

  const info = await rpc.getBlockDagInfo();
  console.info("BlockDagInfo:", info);

  // Start from node sink / pruning point
  let lowHash = info.sink;
  console.info("Starting lowHash (sink):", lowHash);

  await delay(1000);

  // Main loop - runs forever every 10 seconds
  while (true) {
    try {
      const date = new Date();
      const vspc = await rpc.getVirtualChainFromBlockV2({
        startHash: lowHash,
        minConfirmationCount: 10,
        dataVerbosityLevel: "High",
      });
      console.info("VSPC Info:", vspc);

      for (const hash of vspc.removedChainBlockHashes) {
        console.info("Removed block hash:", hash);
      }

      for (const hash of vspc.addedChainBlockHashes) {
        console.info("Added block hash:", hash);
        lowHash = hash;
      }

      for (const cbat of vspc.chainBlockAcceptedTransactions) {
        // Do something with the chain block header
        console.info(cbat.chainBlockHeader);
        // Do something with the accepted transactions
        console.info(cbat.acceptedTransactions);
      }

      console.info("Time span:", Date.now() - date.getTime(), "ms");
    } catch (innerErr) {
      console.error("Error in loop iteration:", innerErr);
      // keep running despite errors
    }

    // wait 10 seconds before next iteration
    await delay(10000);
  }
})();
