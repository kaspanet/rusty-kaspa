// node get-seq-commit-lane-proof.js <lane_key> [chain_block_hash] [--rpc-url=URL] [--network-id=NETWORK]
globalThis.WebSocket ??= require("websocket").w3cwebsocket;

const { RpcClient, Encoding, verifySeqCommitLaneProof } = require("../../../../nodejs/kaspa");
const { values, positionals } = require("node:util").parseArgs({
  allowPositionals: true,
  options: {
    "rpc-url": { type: "string", default: "127.0.0.1" },
    "network-id": { type: "string", default: "mainnet" },
  },
});
const [laneKey, blockHash] = positionals;
const isHash = (value) => /^[0-9a-fA-F]{64}$/.test(value);

if (
  positionals.length < 1 ||
  positionals.length > 2 ||
  !isHash(laneKey) ||
  (blockHash !== undefined && !isHash(blockHash))
) {
  throw new Error(
    "Usage: node get-seq-commit-lane-proof.js <lane_key> [chain_block_hash] [--rpc-url=URL] [--network-id=NETWORK]",
  );
}

// Lane key of a subnetwork: seqCommitLaneKey(subnetworkId). The coinbase lane
// (always active) is 8aa78027db66a16cb69692ee0af5cb76738ef80ad14c9d13920d7fa3cc40b9e4.
// The block must be a selected-chain block; defaults to the sink.
// A real client follows the selected chain itself and verifies against the
// header it already has. This demo takes the block and its header from the
// node being checked, so it only checks consistency with that node.

(async () => {
  const rpc = new RpcClient({
    url: values["rpc-url"],
    encoding: Encoding.Borsh,
    networkId: values["network-id"],
  });
  try {
    await rpc.connect();
    const block = blockHash ?? (await rpc.getBlockDagInfo()).sink;
    const { header } = (await rpc.getBlock({ hash: block, includeTransactions: false })).block;
    const proof = await rpc.getSeqCommitLaneProof({ blockHash: block, laneKey });
    console.dir({ blockHash: block, ...proof }, { depth: null });

    // Throws unless the proof matches the header's commitment.
    const lane = verifySeqCommitLaneProof({ proof, laneKey, seqCommit: header.acceptedIdMerkleRoot });
    console.log(lane ? "verified: lane has an entry" : "verified: lane has no entry");
  } finally {
    await rpc.disconnect();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
