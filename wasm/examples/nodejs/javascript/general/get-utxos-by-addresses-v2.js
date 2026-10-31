// node get-utxos-by-addresses-v2.js <mainnet-address> [rpc-url]
globalThis.WebSocket ??= require("websocket").w3cwebsocket;

const {
  RpcClient,
  Encoding,
  UtxoEntryReference,
} = require("../../../../nodejs/kaspa");

const address = process.argv[2];
const url = process.argv[3] ?? "127.0.0.1";

if (!address) {
  throw new Error(
    "Usage: node get-utxos-by-addresses-v2.js <mainnet-address> [rpc-url]",
  );
}

(async () => {
  const rpc = new RpcClient({
    url,
    encoding: Encoding.Borsh,
    networkId: "mainnet",
  });
  try {
    await rpc.connect();

    const first = await rpc.getUtxosByAddressesV2({
      addresses: [address],
      limit: 2n,
    });
    for (const entry of first.entries) {
      if (!(entry instanceof UtxoEntryReference)) {
        throw new Error("Expected a UtxoEntryReference in the V2 response");
      }
      console.log(entry.outpoint, entry.amount);
    }

    if (first.nextCursor) {
      console.log("Next cursor:", first.nextCursor);
      const second = await rpc.getUtxosByAddressesV2({
        addresses: [address],
        cursor: first.nextCursor,
        limit: 2n,
      });
      console.log("Second page entries:", second.entries.length);
    }

    const shorthand = await rpc.getUtxosByAddressesV2([address]);
    console.log("Array request entries:", shorthand.entries.length);
  } finally {
    await rpc.disconnect();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
