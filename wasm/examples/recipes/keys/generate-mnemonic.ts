// Generate a BIP39 mnemonic and restore it from its phrase.
//
//   cd wasm/examples
//   npx tsx recipes/keys/generate-mnemonic.ts

import { Mnemonic, initConsolePanicHook } from 'kaspa';

initConsolePanicHook();

// A fresh phrase from the platform's secure RNG.
const mnemonic = Mnemonic.random();
console.log('mnemonic:', mnemonic.phrase);

// Restoring from the same phrase reproduces the same seed...
const seed = new Mnemonic(mnemonic.phrase).toSeed();
console.log('seed:    ', seed);

// ...and an optional passphrase (the BIP39 "25th word") derives a different one.
const seedWithPassphrase = mnemonic.toSeed('optional passphrase');
console.log('seed+pw: ', seedWithPassphrase);
