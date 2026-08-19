// Build a pay-to-script-hash (P2SH) locking script, derive its address, and
// produce the unlocking signature script. Pure offline scripting — no network.
//
//   cd wasm/examples
//   npx tsx recipes/transactions/pay-to-script-hash.ts
//   npx tsx recipes/transactions/pay-to-script-hash.ts --network=mainnet

import { ScriptBuilder, Opcodes, addressFromScriptPublicKey, initConsolePanicHook } from 'kaspa';
import { getNetworkId } from '../../shared/network';

initConsolePanicHook();

const network = getNetworkId();

// The redeem script defines the spend condition. `OpTrue` is the simplest one:
// an "anyone-can-spend" script, handy for demos and tests.
const redeemScript = new ScriptBuilder().addOp(Opcodes.OpTrue);
console.log('redeem script:', redeemScript.toString());

// Wrap the redeem script in a P2SH script, then turn that into a payable address.
const p2sh = redeemScript.createPayToScriptHashScript();
const address = addressFromScriptPublicKey(p2sh, network);
console.log('p2sh address: ', address?.toString());

// Spending from that address requires an input whose signature script satisfies
// the redeem script. OpTrue needs no signature, so the unlock script is empty.
// For scripts that do require a signature, sign the input's script hash with
// `signScriptHash(scriptHash, privateKey)` and pass it here instead.
console.log('unlock script:', redeemScript.encodePayToScriptHashSignatureScript(''));
