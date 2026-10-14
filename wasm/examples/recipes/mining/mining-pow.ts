// Construct a block header and check proof-of-work against a nonce.
//
//   cd wasm/examples
//   npx tsx recipes/mining/mining-pow.ts

import { Header, PoW, initConsolePanicHook } from 'kaspa';

initConsolePanicHook();

const blueWork = BigInt('0x' + 'a000000000000001' + 'b000000000000002' + 'c000000000000003');

const header = new Header({
    version: 0,
    parentsByLevel: [['0000000000000000000000000000000000000000000000000000000000000000']],
    hashMerkleRoot: '5510d0c31d6ae3491d6ce8af8e1048c3f287d9c47e4361bd21a9a5fb033a0c1a',
    acceptedIdMerkleRoot: '0000000000000000000000000000000000000000000000000000000000000000',
    utxoCommitment: '0000000000000000000000000000000000000000000000000000000000000000',
    timestamp: 0n,
    bits: 0,
    nonce: 0n,
    daaScore: 0n,
    blueWork,
    blueScore: 0n,
    pruningPoint: '0000000000000000000000000000000000000000000000000000000000000000',
});

// finalize() computes the header hash.
console.log('header hash:', header.finalize());

const pow = new PoW(header);
const [meetsTarget, value] = pow.checkWork(BigInt('0xffffffffffffffff'));
console.log('meets target:', meetsTarget);
console.log('pow value:   ', value.toString(16));
