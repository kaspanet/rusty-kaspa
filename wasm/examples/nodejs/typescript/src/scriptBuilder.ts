import { ScriptBuilder, Opcodes, addressFromScriptPublicKey, NetworkType } from "../../../../nodejs/kaspa"

// An OpTrue is an always spendable script
const myScript = new ScriptBuilder()
                .addOp(Opcodes.OpTrue)

const P2SHScript = myScript.createPayToScriptHashScript()
const address = addressFromScriptPublicKey(P2SHScript, NetworkType.Mainnet)

// Payable address
console.log(address!.toString())
// Unlock signature script
console.log(myScript.encodePayToScriptHashSignatureScript(""))