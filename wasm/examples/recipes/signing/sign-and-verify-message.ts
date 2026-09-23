// Sign an arbitrary message with a private key and verify the signature.
//
//   cd wasm/examples
//   npx tsx recipes/signing/sign-and-verify-message.ts

import { PrivateKey, signMessage, verifyMessage, initConsolePanicHook } from 'kaspa';

initConsolePanicHook();

// BIP340 test vector — safe to hardcode because it controls no funds.
const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef');
const publicKey = privateKey.toPublicKey().toString();

const message = 'Hello Kaspa!';

const signature = signMessage({ message, privateKey });
console.log('signature:', signature);

const verified = verifyMessage({ message, signature, publicKey });
console.log('verified: ', verified);
