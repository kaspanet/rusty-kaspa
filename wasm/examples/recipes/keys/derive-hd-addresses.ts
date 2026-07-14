// Derive HD addresses from a mnemonic (BIP32/BIP44, Kaspa coin type 111111).
//
//   cd wasm/examples
//   npx tsx recipes/keys/derive-hd-addresses.ts
//   KASPA_MNEMONIC="<12 or 24 words>" npx tsx recipes/keys/derive-hd-addresses.ts
//   npx tsx recipes/keys/derive-hd-addresses.ts --network=mainnet

import { XPrv, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';
import { mnemonicOrEphemeral } from '../../shared/secrets';

initConsolePanicHook();

const network = getNetworkId();
const { mnemonic, ephemeral } = mnemonicOrEphemeral();
if (ephemeral) {
    console.log('(using a throwaway mnemonic — set KASPA_MNEMONIC to use your own)\n');
}

// seed -> master extended private key
const xprv = new XPrv(mnemonic.toSeed());

// BIP44 path: m / 44' / 111111' / account' / change / index
//   change = 0 -> receive addresses, change = 1 -> change addresses
for (let i = 0; i < 3; i++) {
    const receive = xprv.derivePath(`m/44'/111111'/0'/0/${i}`).toXPub().toPublicKey().toAddress(network);
    console.log(`receive[${i}]:`, receive.toString());
}

const change = xprv.derivePath("m/44'/111111'/0'/1/0").toXPub().toPublicKey().toAddress(network);
console.log('change[0]: ', change.toString());
