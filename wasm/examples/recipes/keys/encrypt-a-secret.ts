// Encrypt and decrypt data with XChaCha20-Poly1305 (the cipher the wallet uses
// to protect stored keys at rest).
//
//   cd wasm/examples
//   npx tsx recipes/keys/encrypt-a-secret.ts

import { encryptXChaCha20Poly1305, decryptXChaCha20Poly1305, initConsolePanicHook } from 'kaspa';

initConsolePanicHook();

// In a real app the password comes from the user or the environment, never a literal.
const password = 'correct horse battery staple';

const encrypted = encryptXChaCha20Poly1305('a message worth protecting', password);
console.log('encrypted:', encrypted);

const decrypted = decryptXChaCha20Poly1305(encrypted, password);
console.log('decrypted:', decrypted);
