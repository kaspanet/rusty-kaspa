// Turn a key into a Kaspa address.
//
//   cd wasm/examples
//   npx tsx recipes/keys/address-from-public-key.ts
//   npx tsx recipes/keys/address-from-public-key.ts --network=mainnet
//
// All hardcoded keys below are public test vectors that control no funds.

import { PrivateKey, PublicKey, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

const network = getNetworkId();

// A private key yields a keypair, which yields the address.
const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');
console.log('from private key:', privateKey.toKeypair().toAddress(network).toString());

// The same public key in compressed and x-only encodings maps to one address.
const compressed = new PublicKey('02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659');
const xOnly = new PublicKey('dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659');
console.log('compressed pubkey:', compressed.toAddress(network).toString());
console.log('x-only pubkey:    ', xOnly.toAddress(network).toString());
