const kaspa = require('../../../../nodejs/kaspa');
const {parseArgs} = require("../utils");
kaspa.initConsolePanicHook();

(async () => {
    const {} = parseArgs();

    const header = new kaspa.Header({
        // hash : "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af",
        hashMerkleRoot: "bbb490cbce5dc392608000d3aa40e2bfb814c415eac7788237f4eb3467b82059",
        acceptedIdMerkleRoot: "ab7f8fd73cc7f55c3598de5cdd27ef697161879c3edf52488f2ce23054a3e2ed",
        utxoCommitment: "2c2d36bf20940ae59858af89a5acba841cafaf84722174e9136077d3c79d9a44",
        pruningPoint: "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af",
        timestamp: 1n,
        version: 1,
        parentsByLevel: [
            ["b4797d2df131d22b2fed8c61b8de2f6cb7b46c303473be4c36499f7ea96b0a5a"],
            ["dbe16955b7c0e56196cdcdcf3a122681871e7aede1f29ad9ea4f640670483cde"]
        ],
        bits: 23,
        nonce: 567n,
        daaScore: 0n,
        blueScore: 0n,
        blueWork: "baadf00d", // or 12345n
    });

    console.log("initial header:", header);
    const hash = header.finalize();
    console.log("header (after finalize):", header);
    console.log("resulting hash:", hash);

    // header.hash = "73fec18005560d4e3654b1c563c6629d48f3a45f42e5ea772e3ad984339f1e19";

    // note that asJSON() returns a JSON string where each BigInt is represented by an integer value,
    // whereas toJSON() returns a JavaScript object containing BigInt objects.
    const headerAsJSON = header.asJSON();
    console.log("header JSON (via asJSON()):", typeof headerAsJSON, headerAsJSON);
    const headerToJSON = header.toJSON();
    console.log("header JSON (via Serde):", typeof headerToJSON, headerToJSON);

    const header_copy = new kaspa.Header(header);
    header_copy.version = 2;
    header_copy.finalize();
    console.log("header copy:", header_copy);

    // NOTE: that the assignment below has no effect. This is due to `parentsByLevel` being
    // accessed via a `getter/setter`.
    header.parentsByLevel[0][0] = "0000000000000000000000000000000000000000000000000000000000000000";
    console.log("header.parentsByLevel[0][0]:", header.parentsByLevel[0][0]);

    // To change this property, you need to update the entire
    // `parentsByLevel` property in the header via re-assignment.
    const parentsByLevel = header.parentsByLevel;
    parentsByLevel[0][0] = "0000000000000000000000000000000000000000000000000000000000000000";
    header.parentsByLevel = parentsByLevel;
    console.log("header.parentsByLevel:", header.parentsByLevel);
})();
