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

// The lane key is the lane's key in the active-lanes SMT (KIP-21); compute it
// from a subnetwork ID with `seqCommitLaneKey(subnetworkId)`. The coinbase lane
// is active in every chain block, so its key always returns an inclusion proof:
//   8aa78027db66a16cb69692ee0af5cb76738ef80ad14c9d13920d7fa3cc40b9e4
// The block must be a chain (selected-parent-chain) block; the sink is used
// when none is given.

(async () => {
  const rpc = new RpcClient({
    url: values["rpc-url"],
    encoding: Encoding.Borsh,
    networkId: values["network-id"],
  });
  try {
    await rpc.connect();
    const block = blockHash ?? (await rpc.getBlockDagInfo()).sink;
    const proof = await rpc.getSeqCommitLaneProof({ blockHash: block, laneKey });
    console.dir({ blockHash: block, ...proof }, { depth: null });

    // Check the witness against the block header's seq_commit
    // (`acceptedIdMerkleRoot`) instead of trusting the node. In a light client
    // the header itself should be validated first.
    const { header } = (await rpc.getBlock({ hash: block, includeTransactions: false })).block;
    const verified = verifySeqCommitLaneProof({ proof, laneKey, seqCommit: header.acceptedIdMerkleRoot });
    console.log(verified.active ? "verified: lane is active" : "verified: lane is not active (non-inclusion proof)");
  } finally {
    await rpc.disconnect();
  }
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
