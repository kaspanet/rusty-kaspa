const kaspa = require('../../../../nodejs/kaspa');
const {parseArgs} = require("../utils");
kaspa.initConsolePanicHook();

(async () => {
    const {} = parseArgs();

    console.log("creating header");

    const blueWork = BigInt("0x" + "a000000000000001" +
        "b000000000000002" +
        "c000000000000003");
    console.log("blueWork:", blueWork);
    const header = new kaspa.Header({
        version: 0,
        parentsByLevel: [["0000000000000000000000000000000000000000000000000000000000000000"]],
        hashMerkleRoot: "5510d0c31d6ae3491d6ce8af8e1048c3f287d9c47e4361bd21a9a5fb033a0c1a",
        acceptedIdMerkleRoot: "0000000000000000000000000000000000000000000000000000000000000000",
        utxoCommitment: "0000000000000000000000000000000000000000000000000000000000000000",
        timestamp: 0n,
        bits: 0,
        nonce: 0n,
        daaScore: 0n,
        blueWork,
        blueScore: 0n,
        pruningPoint: "0000000000000000000000000000000000000000000000000000000000000000",
    });

    const header_hash = header.finalize();
    console.log("header:", header);
    console.log("header_hash:", header_hash);
    console.log("header.blueWork:", header.blueWork);
    console.log("header.blueWork.toString(16):", header.blueWork.toString(16));

    console.log("creating PoW");
    const pow = new kaspa.PoW(header);
    const nonce = BigInt("0xffffffffffffffff");
    console.log("nonce:", nonce);
    const [a, v] = pow.checkWork(nonce);
    console.log("pow:", pow);
    console.log("[a,v]:", a, v);
    console.log("v.toString(16):", v.toString(16));
})();

