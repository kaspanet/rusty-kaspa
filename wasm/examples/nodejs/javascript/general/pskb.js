// @ts-ignore
globalThis.WebSocket = require("websocket").w3cwebsocket; // W3C WebSocket module shim

const kaspa = require("../../../../nodejs/kaspa");
const { parseArgs } = require("../utils");
const {
  PSKT,
  PSKB,
  TransactionInput,
  Hash,
  payToAddressScript,
  TransactionOutput,
  ScriptPublicKey,
  Address,
} = kaspa;

kaspa.initConsolePanicHook();
(async () => {
  // Your UTXO
  const utxo = {
    outpoint: {
      transactionId: new Hash(
        "0e111d1b7116364b8c729e10863719ea790addcc3e9deff97894b23e07f1e741",
      ),
      index: 0,
    },
    amount: 9799885899n,
    scriptPublicKey: new ScriptPublicKey(
      0,
      "0000202d8a1414e62e081fb6bcf644e648c18061c2855575cac722f86324cad91dd0faac",
    ),
    blockDaaScore: 84746196n,
    isCoinbase: false,
    address: new Address(
      "kaspatest:qqkc59q5uchqs8akhnmyfejgcxqxrs59246u43ezlp3jfjkerhg05e3rk3cwf",
    ),
  };

  // COMMIT TRANSACTION
  let commitPskt = new PSKT(undefined);
  commitPskt.inputsModifiable();
  commitPskt.outputsModifiable();
  commitPskt = commitPskt.toConstructor();

  // Add input
  commitPskt.input(
    new TransactionInput({
      previousOutpoint: {
        transactionId: utxo.outpoint.transactionId.toString(),
        index: utxo.outpoint.index,
      },
      sequence: 0n,
      sigOpCount: 1,
      utxo: {
        outpoint: {
          transactionId: utxo.outpoint.transactionId.toString(),
          index: utxo.outpoint.index,
        },
        amount: utxo.amount,
        scriptPublicKey: utxo.scriptPublicKey,
        blockDaaScore: utxo.blockDaaScore,
        isCoinbase: utxo.isCoinbase,
        address: utxo.address,
      },
      signatureScript: "",
    }),
  );

  // Add P2SH output (1 KAS)
  const p2shScript = payToAddressScript(utxo.address);
  commitPskt.output(
    new TransactionOutput(
      100000000n,
      new ScriptPublicKey(0, p2shScript.script),
    ),
  );

  // Add change output
  const changeScript = payToAddressScript(utxo.address);
  commitPskt.output(
    new TransactionOutput(
      9699885899n,
      new ScriptPublicKey(0, changeScript.script),
    ),
  );

  // Get commit transaction ID for reveal
  commitPskt = commitPskt.toSigner();
  const commitTxId = commitPskt.calculateId();
  console.log("Commit transaction ID:", commitTxId.toString());
  // REVEAL TRANSACTION
  let revealPskt = new PSKT(undefined);
  revealPskt.inputsModifiable();
  revealPskt.outputsModifiable();
  revealPskt = revealPskt.toConstructor();

  // Use commit tx ID for reveal input
  const commitOutpoint = {
    transactionId: commitTxId.toString(),
    index: 0, // First output from commit tx (the P2SH output)
  };

  // Add input from P2SH with scriptSig
  revealPskt.input(
    new TransactionInput({
      previousOutpoint: commitOutpoint,
      sequence: 0n,
      sigOpCount: 1,
      utxo: {
        outpoint: commitOutpoint,
        amount: 100000000n,
        scriptPublicKey: new ScriptPublicKey(0, p2shScript.script),
        blockDaaScore: 18446744073709551615n, // u64::MAX for reveal transaction
        isCoinbase: false,
        address: utxo.address,
      },
      signatureScript: "",
    }),
  );

  // Add output to reveal address
  const revealScript = payToAddressScript(
    "kaspatest:qpurs0zsder98a7mr285nxypnuc9nlcsadkt63agjazx9h0jl0x47njwssfq9",
  );
  revealPskt.output(
    new TransactionOutput(
      100000000n,
      new ScriptPublicKey(0, revealScript.script),
    ),
  );

  // Create single PSKB with both transactions
  let pskb = new PSKB();
  pskb.add(commitPskt);
  pskb.add(revealPskt);

  // console.log("Combined PSKB:", pskb.serialize());

  for (let i = 0; i < pskb.length; i++) {
    console.log(`pskt ${i}/${pskb.length}`, pskb.get(i));
  }

  console.log("bye!");
})();
