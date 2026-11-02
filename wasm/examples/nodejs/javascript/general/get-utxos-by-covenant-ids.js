// node get-utxos-by-covenant-ids.js <covenant-id[,covenant-id...]> [rpc-url] [network-id]
// Start the node with --utxoindex --covenant-index and enable Borsh wRPC.
globalThis.WebSocket ??= require("websocket").w3cwebsocket;

const {
  RpcClient,
  Encoding,
  UtxoEntryReference,
} = require("../../../../nodejs/kaspa");

const covenantIds = process.argv[2]?.split(",");
const url = process.argv[3] ?? "127.0.0.1";
const networkId = process.argv[4] ?? "mainnet";

if (!covenantIds?.length || covenantIds.some((id) => !/^[0-9a-fA-F]{64}$/.test(id))) {
  throw new Error(
    "Usage: node get-utxos-by-covenant-ids.js <covenant-id[,covenant-id...]> [rpc-url] [network-id]",
  );
}

(async () => {
  const rpc = new RpcClient({ url, encoding: Encoding.Borsh, networkId });
  try {
    await rpc.connect();

    // Keep the same IDs and filters for every page; the node sorts the IDs.
    const query = {
      covenantIds,
      scriptPublicKeys: [], // Empty includes all scripts; accepts script objects or hex strings.
      // fromDaaScore: 0n,
      // toDaaScore: 1000000n,
      budget: 1000n, // Weighted database operations, not an entry count.
    };
    let cursor;
    let page = 0;
    let totalEntries = 0;
    let totalAmount = 0n;
    do {
      const response = await rpc.getUtxosByCovenantIds({ ...query, cursor });
      for (const entry of response.entries) {
        if (!(entry instanceof UtxoEntryReference)) {
          throw new Error("Expected a UtxoEntryReference in the response");
        }
        console.log(entry.outpoint, entry.amount);
        totalAmount += entry.amount;
      }
      totalEntries += response.entries.length;
      console.log(`Page ${++page}: ${response.entries.length} entries`);

      // A budget can be exhausted before a match is found. Empty pages can
      // still have a cursor: only stop when the node returns no nextCursor.
      cursor = response.nextCursor;
    } while (cursor);

    console.log(`Complete: ${totalEntries} UTXOs, ${totalAmount} sompi`);
  } finally {
    await rpc.disconnect();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
