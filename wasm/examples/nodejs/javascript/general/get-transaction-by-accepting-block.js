// node get-transaction-by-accepting-block.js <accepting_block_hash_or_daa> <txid[,txid...]> [--rpc-url=URL] [--network-id=NETWORK]
globalThis.WebSocket ??= require("websocket").w3cwebsocket;

const { RpcClient, Encoding } = require("../../../../nodejs/kaspa");
const { values, positionals } = require("node:util").parseArgs({
  allowPositionals: true,
  options: {
    "rpc-url": { type: "string", default: "127.0.0.1" },
    "network-id": { type: "string", default: "mainnet" },
  },
});
const [block, ids] = positionals;
const transactionIds = ids?.split(",");

if (
  positionals.length !== 2 ||
  !/^(?:[0-9a-fA-F]{64}|\d+)$/.test(block) ||
  transactionIds.some((id) => !/^[0-9a-fA-F]{64}$/.test(id))
) {
  throw new Error(
    "Usage: node get-transaction-by-accepting-block.js <accepting_block_hash_or_daa> <txid[,txid...]> [--rpc-url=URL] [--network-id=NETWORK]",
  );
}

// For a UTXO, use its entry's DAA score and its outpoint's transaction ID.
// The accepting score must belong to the current virtual chain. Small reorgs
// may require waiting 1-2 seconds or refreshing the UTXO data before retrying.
const acceptingBlock = /^[0-9a-fA-F]{64}$/.test(block)
  ? { hash: block }
  : { daaScore: BigInt(block) };

(async () => {
  const rpc = new RpcClient({
    url: values["rpc-url"],
    encoding: Encoding.Borsh,
    networkId: values["network-id"],
  });
  try {
    await rpc.connect();
    console.dir(
      await rpc.getTransactionsByAcceptingBlock({
        acceptingBlock,
        transactionIds,
      }),
      { depth: null },
    );
  } finally {
    await rpc.disconnect();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
