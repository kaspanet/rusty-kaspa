// Build, finalize, and inspect a block header.
//
//   cd wasm/examples
//   npx tsx recipes/mining/mining-header.ts

import { Header, initConsolePanicHook } from 'kaspa';

initConsolePanicHook();

const header = new Header({
    version: 1,
    hashMerkleRoot: 'bbb490cbce5dc392608000d3aa40e2bfb814c415eac7788237f4eb3467b82059',
    acceptedIdMerkleRoot: 'ab7f8fd73cc7f55c3598de5cdd27ef697161879c3edf52488f2ce23054a3e2ed',
    utxoCommitment: '2c2d36bf20940ae59858af89a5acba841cafaf84722174e9136077d3c79d9a44',
    pruningPoint: '8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af',
    timestamp: 1n,
    parentsByLevel: [
        ['b4797d2df131d22b2fed8c61b8de2f6cb7b46c303473be4c36499f7ea96b0a5a'],
        ['dbe16955b7c0e56196cdcdcf3a122681871e7aede1f29ad9ea4f640670483cde'],
    ],
    bits: 23,
    nonce: 567n,
    daaScore: 0n,
    blueScore: 0n,
    blueWork: 'baadf00d',
});

console.log('hash:', header.finalize());

// asJSON() encodes BigInts as integers; toJSON() returns BigInt objects.
console.log('asJSON():', header.asJSON());

// Header fields are exposed as getters/setters. Reassign a whole property to
// change it (mutating a returned array in place has no effect).
const copy = new Header(header);
copy.version = 2;
console.log('copy hash (version 2):', copy.finalize());
