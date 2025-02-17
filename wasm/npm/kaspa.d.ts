/* tslint:disable */
/* eslint-disable */
/**
* Returns true if the script passed is a pay-to-script-hash (P2SH) format, false otherwise.
* @param script - The script ({@link HexString} or Uint8Array).
* @category Wallet SDK
* @param {HexString | Uint8Array} script
* @returns {boolean}
*/
export function isScriptPayToScriptHash(script: HexString | Uint8Array): boolean;
/**
* Returns returns true if the script passed is an ECDSA pay-to-pubkey.
* @param script - The script ({@link HexString} or Uint8Array).
* @category Wallet SDK
* @param {HexString | Uint8Array} script
* @returns {boolean}
*/
export function isScriptPayToPubkeyECDSA(script: HexString | Uint8Array): boolean;
/**
* Returns true if the script passed is a pay-to-pubkey.
* @param script - The script ({@link HexString} or Uint8Array).
* @category Wallet SDK
* @param {HexString | Uint8Array} script
* @returns {boolean}
*/
export function isScriptPayToPubkey(script: HexString | Uint8Array): boolean;
/**
* Returns the address encoded in a script public key.
* @param script_public_key - The script public key ({@link ScriptPublicKey}).
* @param network - The network type.
* @category Wallet SDK
* @param {ScriptPublicKey | HexString} script_public_key
* @param {NetworkType | NetworkId | string} network
* @returns {Address | undefined}
*/
export function addressFromScriptPublicKey(script_public_key: ScriptPublicKey | HexString, network: NetworkType | NetworkId | string): Address | undefined;
/**
* Generates a signature script that fits a pay-to-script-hash script.
* @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
* @param signature - The signature ({@link HexString} or Uint8Array).
* @category Wallet SDK
* @param {HexString | Uint8Array} redeem_script
* @param {HexString | Uint8Array} signature
* @returns {HexString}
*/
export function payToScriptHashSignatureScript(redeem_script: HexString | Uint8Array, signature: HexString | Uint8Array): HexString;
/**
* Takes a script and returns an equivalent pay-to-script-hash script.
* @param redeem_script - The redeem script ({@link HexString} or Uint8Array).
* @category Wallet SDK
* @param {HexString | Uint8Array} redeem_script
* @returns {ScriptPublicKey}
*/
export function payToScriptHashScript(redeem_script: HexString | Uint8Array): ScriptPublicKey;
/**
* Creates a new script to pay a transaction output to the specified address.
* @category Wallet SDK
* @param {Address | string} address
* @returns {ScriptPublicKey}
*/
export function payToAddressScript(address: Address | string): ScriptPublicKey;
/**
* Calculates target from difficulty, based on set_difficulty function on
* <https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L375>
* @category Mining
* @param {number} difficulty
* @returns {bigint}
*/
export function calculateTarget(difficulty: number): bigint;
/**
* Helper function that creates an estimate using the transaction {@link Generator}
* by producing only the {@link GeneratorSummary} containing the estimate.
* @see {@link IGeneratorSettingsObject}, {@link Generator}, {@link createTransactions}
* @category Wallet SDK
* @param {IGeneratorSettingsObject} settings
* @returns {Promise<GeneratorSummary>}
*/
export function estimateTransactions(settings: IGeneratorSettingsObject): Promise<GeneratorSummary>;
/**
* Helper function that creates a set of transactions using the transaction {@link Generator}.
* @see {@link IGeneratorSettingsObject}, {@link Generator}, {@link estimateTransactions}
* @category Wallet SDK
* @param {IGeneratorSettingsObject} settings
* @returns {Promise<ICreateTransactions>}
*/
export function createTransactions(settings: IGeneratorSettingsObject): Promise<ICreateTransactions>;
/**
* Create a basic transaction without any mass limit checks.
* @category Wallet SDK
* @param {IUtxoEntry[]} utxo_entry_source
* @param {IPaymentOutput[]} outputs
* @param {bigint} priority_fee
* @param {HexString | Uint8Array | undefined} [payload]
* @param {number | undefined} [sig_op_count]
* @returns {Transaction}
*/
export function createTransaction(utxo_entry_source: IUtxoEntry[], outputs: IPaymentOutput[], priority_fee: bigint, payload?: HexString | Uint8Array, sig_op_count?: number): Transaction;
/**
* @category Wallet SDK
* @param {PublicKey | string} key
* @param {NetworkType | NetworkId | string} network
* @param {boolean | undefined} [ecdsa]
* @param {AccountKind | undefined} [account_kind]
* @returns {Address}
*/
export function createAddress(key: PublicKey | string, network: NetworkType | NetworkId | string, ecdsa?: boolean, account_kind?: AccountKind): Address;
/**
* @category Wallet SDK
* @param {number} minimum_signatures
* @param {(PublicKey | string)[]} keys
* @param {NetworkType} network_type
* @param {boolean | undefined} [ecdsa]
* @param {AccountKind | undefined} [account_kind]
* @returns {Address}
*/
export function createMultisigAddress(minimum_signatures: number, keys: (PublicKey | string)[], network_type: NetworkType, ecdsa?: boolean, account_kind?: AccountKind): Address;
/**
* @category Wallet SDK
* @param {any} script_hash
* @param {PrivateKey} privkey
* @returns {string}
*/
export function signScriptHash(script_hash: any, privkey: PrivateKey): string;
/**
* `createInputSignature()` is a helper function to sign a transaction input with a specific SigHash type using a private key.
* @category Wallet SDK
* @param {Transaction} tx
* @param {number} input_index
* @param {PrivateKey} private_key
* @param {SighashType | undefined} [sighash_type]
* @returns {HexString}
*/
export function createInputSignature(tx: Transaction, input_index: number, private_key: PrivateKey, sighash_type?: SighashType): HexString;
/**
* `signTransaction()` is a helper function to sign a transaction using a private key array or a signer array.
* @category Wallet SDK
* @param {Transaction} tx
* @param {(PrivateKey | HexString | Uint8Array)[]} signer
* @param {boolean} verify_sig
* @returns {Transaction}
*/
export function signTransaction(tx: Transaction, signer: (PrivateKey | HexString | Uint8Array)[], verify_sig: boolean): Transaction;
/**
*
* Format a Sompi amount to a string representation of the amount in Kaspa with a suffix
* based on the network type (e.g. `KAS` for mainnet, `TKAS` for testnet,
* `SKAS` for simnet, `DKAS` for devnet).
*
* @category Wallet SDK
* @param {bigint | number | HexString} sompi
* @param {NetworkType | NetworkId | string} network
* @returns {string}
*/
export function sompiToKaspaStringWithSuffix(sompi: bigint | number | HexString, network: NetworkType | NetworkId | string): string;
/**
*
* Convert Sompi to a string representation of the amount in Kaspa.
*
* @category Wallet SDK
* @param {bigint | number | HexString} sompi
* @returns {string}
*/
export function sompiToKaspaString(sompi: bigint | number | HexString): string;
/**
* Convert a Kaspa string to Sompi represented by bigint.
* This function provides correct precision handling and
* can be used to parse user input.
* @category Wallet SDK
* @param {string} kaspa
* @returns {bigint | undefined}
*/
export function kaspaToSompi(kaspa: string): bigint | undefined;
/**
* Set a custom storage folder for the wallet SDK
* subsystem.  Encrypted wallet files and transaction
* data will be stored in this folder. If not set
* the storage folder will default to `~/.kaspa`
* (note that the folder is hidden).
*
* This must be called before using any other wallet
* SDK functions.
*
* NOTE: This function will create a folder if it
* doesn't exist. This function will have no effect
* if invoked in the browser environment.
*
* @param {String} folder - the path to the storage folder
*
* @category Wallet API
*/
export function setDefaultStorageFolder(folder: string): void;
/**
* Set the name of the default wallet file name
* or the `localStorage` key.  If `Wallet::open`
* is called without a wallet file name, this name
* will be used.  Please note that this name
* will be suffixed with `.wallet` suffix.
*
* This function should be called before using any
* other wallet SDK functions.
*
* @param {String} folder - the name to the wallet file or key.
*
* @category Wallet API
* @param {string} folder
*/
export function setDefaultWalletFile(folder: string): void;
/**
* Verifies with a public key the signature of the given message
* @category Message Signing
*/
export function verifyMessage(value: IVerifyMessage): boolean;
/**
* Signs a message with the given private key
* @category Message Signing
* @param {ISignMessage} value
* @returns {HexString}
*/
export function signMessage(value: ISignMessage): HexString;
/**
* `calculateStorageMass()` is a helper function to compute the storage mass of inputs and outputs.
* This function can be use to calculate the storage mass of transaction inputs and outputs.
* Note that the storage mass is only a component of the total transaction mass. You are not
* meant to use this function by itself and should use `calculateTransactionMass()` instead.
* This function purely exists for diagnostic purposes and to help with complex algorithms that
* may require a manual UTXO selection for identifying UTXOs and outputs needed for low storage mass.
*
* @category Wallet SDK
* @see {@link maximumStandardTransactionMass}
* @see {@link calculateTransactionMass}
* @param {NetworkId | string} network_id
* @param {Array<number>} input_values
* @param {Array<number>} output_values
* @returns {bigint | undefined}
*/
export function calculateStorageMass(network_id: NetworkId | string, input_values: Array<number>, output_values: Array<number>): bigint | undefined;
/**
* `calculateTransactionFee()` returns minimum fees needed for the transaction to be
* accepted by the network. If the transaction is invalid or the mass can not be calculated,
* the function throws an error. If the mass exceeds the maximum standard transaction mass,
* the function returns `undefined`.
*
* @category Wallet SDK
* @see {@link maximumStandardTransactionMass}
* @see {@link calculateTransactionMass}
* @see {@link updateTransactionMass}
* @param {NetworkId | string} network_id
* @param {ITransaction | Transaction} tx
* @param {number | undefined} [minimum_signatures]
* @returns {bigint | undefined}
*/
export function calculateTransactionFee(network_id: NetworkId | string, tx: ITransaction | Transaction, minimum_signatures?: number): bigint | undefined;
/**
* `updateTransactionMass()` updates the mass property of the passed transaction.
* If the transaction is invalid, the function throws an error.
*
* The function returns `true` if the mass is within the maximum standard transaction mass and
* the transaction mass is updated. Otherwise, the function returns `false`.
*
* This is similar to `calculateTransactionMass()` but modifies the supplied
* `Transaction` object.
*
* @category Wallet SDK
* @see {@link maximumStandardTransactionMass}
* @see {@link calculateTransactionMass}
* @see {@link calculateTransactionFee}
* @param {NetworkId | string} network_id
* @param {Transaction} tx
* @param {number | undefined} [minimum_signatures]
* @returns {boolean}
*/
export function updateTransactionMass(network_id: NetworkId | string, tx: Transaction, minimum_signatures?: number): boolean;
/**
* `calculateTransactionMass()` returns the mass of the passed transaction.
* If the transaction is invalid, or the mass can not be calculated
* the function throws an error.
*
* The mass value must not exceed the maximum standard transaction mass
* that can be obtained using `maximumStandardTransactionMass()`.
*
* @category Wallet SDK
* @see {@link maximumStandardTransactionMass}
* @param {NetworkId | string} network_id
* @param {ITransaction | Transaction} tx
* @param {number | undefined} [minimum_signatures]
* @returns {bigint}
*/
export function calculateTransactionMass(network_id: NetworkId | string, tx: ITransaction | Transaction, minimum_signatures?: number): bigint;
/**
* `maximumStandardTransactionMass()` returns the maximum transaction
* size allowed by the network.
*
* @category Wallet SDK
* @see {@link calculateTransactionMass}
* @see {@link updateTransactionMass}
* @see {@link calculateTransactionFee}
* @returns {bigint}
*/
export function maximumStandardTransactionMass(): bigint;
/**
* WASM32 binding for `argon2sha256iv` hash function.
* @param text - The text string to hash.
* @category Encryption
* @param {string} text
* @param {number} byteLength
* @returns {HexString}
*/
export function argon2sha256ivFromText(text: string, byteLength: number): HexString;
/**
* WASM32 binding for `argon2sha256iv` hash function.
* @param data - The data to hash ({@link HexString} or Uint8Array).
* @category Encryption
* @param {HexString | Uint8Array} data
* @param {number} hashLength
* @returns {HexString}
*/
export function argon2sha256ivFromBinary(data: HexString | Uint8Array, hashLength: number): HexString;
/**
* WASM32 binding for `SHA256d` hash function.
* @param {string} text - The text string to hash.
* @category Encryption
* @param {string} text
* @returns {HexString}
*/
export function sha256dFromText(text: string): HexString;
/**
* WASM32 binding for `SHA256d` hash function.
* @param data - The data to hash ({@link HexString} or Uint8Array).
* @category Encryption
* @param {HexString | Uint8Array} data
* @returns {HexString}
*/
export function sha256dFromBinary(data: HexString | Uint8Array): HexString;
/**
* WASM32 binding for `SHA256` hash function.
* @param {string} text - The text string to hash.
* @category Encryption
* @param {string} text
* @returns {HexString}
*/
export function sha256FromText(text: string): HexString;
/**
* WASM32 binding for `SHA256` hash function.
* @param data - The data to hash ({@link HexString} or Uint8Array).
* @category Encryption
* @param {HexString | Uint8Array} data
* @returns {HexString}
*/
export function sha256FromBinary(data: HexString | Uint8Array): HexString;
/**
* WASM32 binding for `decryptXChaCha20Poly1305` function.
* @category Encryption
* @param {string} base64string
* @param {string} password
* @returns {string}
*/
export function decryptXChaCha20Poly1305(base64string: string, password: string): string;
/**
* WASM32 binding for `encryptXChaCha20Poly1305` function.
* @returns The encrypted text as a base64 string.
* @category Encryption
* @param {string} plainText
* @param {string} password
* @returns {string}
*/
export function encryptXChaCha20Poly1305(plainText: string, password: string): string;
/**
* Returns the version of the Rusty Kaspa framework.
* @category General
* @returns {string}
*/
export function version(): string;
/**
*Set the logger log level using a string representation.
*Available variants are: 'off', 'error', 'warn', 'info', 'debug', 'trace'
*@category General
* @param {"off" | "error" | "warn" | "info" | "debug" | "trace"} level
*/
export function setLogLevel(level: "off" | "error" | "warn" | "info" | "debug" | "trace"): void;
/**
* Initialize Rust panic handler in console mode.
*
* This will output additional debug information during a panic to the console.
* This function should be called right after loading WASM libraries.
* @category General
*/
export function initConsolePanicHook(): void;
/**
* Initialize Rust panic handler in browser mode.
*
* This will output additional debug information during a panic in the browser
* by creating a full-screen `DIV`. This is useful on mobile devices or where
* the user otherwise has no access to console/developer tools. Use
* {@link presentPanicHookLogs} to activate the panic logs in the
* browser environment.
* @see {@link presentPanicHookLogs}
* @category General
*/
export function initBrowserPanicHook(): void;
/**
* Present panic logs to the user in the browser.
*
* This function should be called after a panic has occurred and the
* browser-based panic hook has been activated. It will present the
* collected panic logs in a full-screen `DIV` in the browser.
* @see {@link initBrowserPanicHook}
* @category General
*/
export function presentPanicHookLogs(): void;
/**
*r" Deferred promise - an object that has `resolve()` and `reject()`
*r" functions that can be called outside of the promise body.
*r" WARNING: This function uses `eval` and can not be used in environments
*r" where dynamically-created code can not be executed such as web browser
*r" extensions.
*r" @category General
* @returns {Promise<any>}
*/
export function defer(): Promise<any>;
/**
* Configuration for the WASM32 bindings runtime interface.
* @see {@link IWASM32BindingsConfig}
* @category General
* @param {IWASM32BindingsConfig} config
*/
export function initWASM32Bindings(config: IWASM32BindingsConfig): void;
/**
* @category Consensus
*/
export enum NetworkType {
  Mainnet = 0,
  Testnet = 1,
  Devnet = 2,
  Simnet = 3,
}
/**
* Specifies the type of an account address to create.
* The address can bea receive address or a change address.
*
* @category Wallet API
*/
export enum NewAddressKind {
  Receive = 0,
  Change = 1,
}
/**
* wRPC protocol encoding: `Borsh` or `JSON`
* @category Transport
*/
export enum Encoding {
  Borsh = 0,
  SerdeJson = 1,
}
/**
* `ConnectionStrategy` specifies how the WebSocket `async fn connect()`
* function should behave during the first-time connectivity phase.
* @category WebSocket
*/
export enum ConnectStrategy {
/**
* Continuously attempt to connect to the server. This behavior will
* block `connect()` function until the connection is established.
*/
  Retry = 0,
/**
* Causes `connect()` to return immediately if the first-time connection
* has failed.
*/
  Fallback = 1,
}
/**
* Kaspa Transaction Script Opcodes
* @see {@link ScriptBuilder}
* @category Consensus
*/
export enum Opcodes {
  OpFalse = 0,
  OpData1 = 1,
  OpData2 = 2,
  OpData3 = 3,
  OpData4 = 4,
  OpData5 = 5,
  OpData6 = 6,
  OpData7 = 7,
  OpData8 = 8,
  OpData9 = 9,
  OpData10 = 10,
  OpData11 = 11,
  OpData12 = 12,
  OpData13 = 13,
  OpData14 = 14,
  OpData15 = 15,
  OpData16 = 16,
  OpData17 = 17,
  OpData18 = 18,
  OpData19 = 19,
  OpData20 = 20,
  OpData21 = 21,
  OpData22 = 22,
  OpData23 = 23,
  OpData24 = 24,
  OpData25 = 25,
  OpData26 = 26,
  OpData27 = 27,
  OpData28 = 28,
  OpData29 = 29,
  OpData30 = 30,
  OpData31 = 31,
  OpData32 = 32,
  OpData33 = 33,
  OpData34 = 34,
  OpData35 = 35,
  OpData36 = 36,
  OpData37 = 37,
  OpData38 = 38,
  OpData39 = 39,
  OpData40 = 40,
  OpData41 = 41,
  OpData42 = 42,
  OpData43 = 43,
  OpData44 = 44,
  OpData45 = 45,
  OpData46 = 46,
  OpData47 = 47,
  OpData48 = 48,
  OpData49 = 49,
  OpData50 = 50,
  OpData51 = 51,
  OpData52 = 52,
  OpData53 = 53,
  OpData54 = 54,
  OpData55 = 55,
  OpData56 = 56,
  OpData57 = 57,
  OpData58 = 58,
  OpData59 = 59,
  OpData60 = 60,
  OpData61 = 61,
  OpData62 = 62,
  OpData63 = 63,
  OpData64 = 64,
  OpData65 = 65,
  OpData66 = 66,
  OpData67 = 67,
  OpData68 = 68,
  OpData69 = 69,
  OpData70 = 70,
  OpData71 = 71,
  OpData72 = 72,
  OpData73 = 73,
  OpData74 = 74,
  OpData75 = 75,
  OpPushData1 = 76,
  OpPushData2 = 77,
  OpPushData4 = 78,
  Op1Negate = 79,
  OpReserved = 80,
  OpTrue = 81,
  Op2 = 82,
  Op3 = 83,
  Op4 = 84,
  Op5 = 85,
  Op6 = 86,
  Op7 = 87,
  Op8 = 88,
  Op9 = 89,
  Op10 = 90,
  Op11 = 91,
  Op12 = 92,
  Op13 = 93,
  Op14 = 94,
  Op15 = 95,
  Op16 = 96,
  OpNop = 97,
  OpVer = 98,
  OpIf = 99,
  OpNotIf = 100,
  OpVerIf = 101,
  OpVerNotIf = 102,
  OpElse = 103,
  OpEndIf = 104,
  OpVerify = 105,
  OpReturn = 106,
  OpToAltStack = 107,
  OpFromAltStack = 108,
  Op2Drop = 109,
  Op2Dup = 110,
  Op3Dup = 111,
  Op2Over = 112,
  Op2Rot = 113,
  Op2Swap = 114,
  OpIfDup = 115,
  OpDepth = 116,
  OpDrop = 117,
  OpDup = 118,
  OpNip = 119,
  OpOver = 120,
  OpPick = 121,
  OpRoll = 122,
  OpRot = 123,
  OpSwap = 124,
  OpTuck = 125,
/**
* Splice opcodes.
*/
  OpCat = 126,
  OpSubStr = 127,
  OpLeft = 128,
  OpRight = 129,
  OpSize = 130,
/**
* Bitwise logic opcodes.
*/
  OpInvert = 131,
  OpAnd = 132,
  OpOr = 133,
  OpXor = 134,
  OpEqual = 135,
  OpEqualVerify = 136,
  OpReserved1 = 137,
  OpReserved2 = 138,
/**
* Numeric related opcodes.
*/
  Op1Add = 139,
  Op1Sub = 140,
  Op2Mul = 141,
  Op2Div = 142,
  OpNegate = 143,
  OpAbs = 144,
  OpNot = 145,
  Op0NotEqual = 146,
  OpAdd = 147,
  OpSub = 148,
  OpMul = 149,
  OpDiv = 150,
  OpMod = 151,
  OpLShift = 152,
  OpRShift = 153,
  OpBoolAnd = 154,
  OpBoolOr = 155,
  OpNumEqual = 156,
  OpNumEqualVerify = 157,
  OpNumNotEqual = 158,
  OpLessThan = 159,
  OpGreaterThan = 160,
  OpLessThanOrEqual = 161,
  OpGreaterThanOrEqual = 162,
  OpMin = 163,
  OpMax = 164,
  OpWithin = 165,
/**
* Undefined opcodes.
*/
  OpUnknown166 = 166,
  OpUnknown167 = 167,
/**
* Crypto opcodes.
*/
  OpSHA256 = 168,
  OpCheckMultiSigECDSA = 169,
  OpBlake2b = 170,
  OpCheckSigECDSA = 171,
  OpCheckSig = 172,
  OpCheckSigVerify = 173,
  OpCheckMultiSig = 174,
  OpCheckMultiSigVerify = 175,
  OpCheckLockTimeVerify = 176,
  OpCheckSequenceVerify = 177,
/**
* Undefined opcodes.
*/
  OpUnknown178 = 178,
  OpUnknown179 = 179,
  OpUnknown180 = 180,
  OpUnknown181 = 181,
  OpUnknown182 = 182,
  OpUnknown183 = 183,
  OpUnknown184 = 184,
  OpUnknown185 = 185,
  OpUnknown186 = 186,
  OpUnknown187 = 187,
  OpUnknown188 = 188,
  OpUnknown189 = 189,
  OpUnknown190 = 190,
  OpUnknown191 = 191,
  OpUnknown192 = 192,
  OpUnknown193 = 193,
  OpUnknown194 = 194,
  OpUnknown195 = 195,
  OpUnknown196 = 196,
  OpUnknown197 = 197,
  OpUnknown198 = 198,
  OpUnknown199 = 199,
  OpUnknown200 = 200,
  OpUnknown201 = 201,
  OpUnknown202 = 202,
  OpUnknown203 = 203,
  OpUnknown204 = 204,
  OpUnknown205 = 205,
  OpUnknown206 = 206,
  OpUnknown207 = 207,
  OpUnknown208 = 208,
  OpUnknown209 = 209,
  OpUnknown210 = 210,
  OpUnknown211 = 211,
  OpUnknown212 = 212,
  OpUnknown213 = 213,
  OpUnknown214 = 214,
  OpUnknown215 = 215,
  OpUnknown216 = 216,
  OpUnknown217 = 217,
  OpUnknown218 = 218,
  OpUnknown219 = 219,
  OpUnknown220 = 220,
  OpUnknown221 = 221,
  OpUnknown222 = 222,
  OpUnknown223 = 223,
  OpUnknown224 = 224,
  OpUnknown225 = 225,
  OpUnknown226 = 226,
  OpUnknown227 = 227,
  OpUnknown228 = 228,
  OpUnknown229 = 229,
  OpUnknown230 = 230,
  OpUnknown231 = 231,
  OpUnknown232 = 232,
  OpUnknown233 = 233,
  OpUnknown234 = 234,
  OpUnknown235 = 235,
  OpUnknown236 = 236,
  OpUnknown237 = 237,
  OpUnknown238 = 238,
  OpUnknown239 = 239,
  OpUnknown240 = 240,
  OpUnknown241 = 241,
  OpUnknown242 = 242,
  OpUnknown243 = 243,
  OpUnknown244 = 244,
  OpUnknown245 = 245,
  OpUnknown246 = 246,
  OpUnknown247 = 247,
  OpUnknown248 = 248,
  OpUnknown249 = 249,
  OpSmallInteger = 250,
  OpPubKeys = 251,
  OpUnknown252 = 252,
  OpPubKeyHash = 253,
  OpPubKey = 254,
  OpInvalidOpCode = 255,
}
/**
*
* Languages supported by BIP39.
*
* Presently only English is specified by the BIP39 standard.
*
* @see {@link Mnemonic}
*
* @category Wallet SDK
*/
export enum Language {
/**
* English is presently the only supported language
*/
  English = 0,
}
/**
*
* @see {@link IFees}, {@link IGeneratorSettingsObject}, {@link Generator}, {@link estimateTransactions}, {@link createTransactions}
* @category Wallet SDK
*/
export enum FeeSource {
  SenderPays = 0,
  ReceiverPays = 1,
}
/**
* Kaspa Sighash types allowed by consensus
* @category Consensus
*/
export enum SighashType {
  All = 0,
  None = 1,
  Single = 2,
  AllAnyOneCanPay = 3,
  NoneAnyOneCanPay = 4,
  SingleAnyOneCanPay = 5,
}
/**
* @category Wallet API
*/
export enum AccountsDiscoveryKind {
  Bip44 = 0,
}
/**
*
*  Kaspa `Address` version (`PubKey`, `PubKey ECDSA`, `ScriptHash`)
*
* @category Address
*/
export enum AddressVersion {
/**
* PubKey addresses always have the version byte set to 0
*/
  PubKey = 0,
/**
* PubKey ECDSA addresses always have the version byte set to 1
*/
  PubKeyECDSA = 1,
/**
* ScriptHash addresses always have the version byte set to 8
*/
  ScriptHash = 8,
}

/**
 * Interface defines the structure of a UTXO entry.
 * 
 * @category Consensus
 */
export interface IUtxoEntry {
    /** @readonly */
    address?: Address;
    /** @readonly */
    outpoint: ITransactionOutpoint;
    /** @readonly */
    amount : bigint;
    /** @readonly */
    scriptPublicKey : IScriptPublicKey;
    /** @readonly */
    blockDaaScore: bigint;
    /** @readonly */
    isCoinbase: boolean;
}




/**
 * Interface defining the structure of a transaction output.
 * 
 * @category Consensus
 */
export interface ITransactionOutput {
    value: bigint;
    scriptPublicKey: IScriptPublicKey | HexString;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionOutputVerboseData;
}

/**
 * TransactionOutput verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionOutputVerboseData {
    scriptPublicKeyType : string;
    scriptPublicKeyAddress : string;
}



/**
 * Interface defining the structure of a transaction.
 * 
 * @category Consensus
 */
export interface ITransaction {
    version: number;
    inputs: ITransactionInput[];
    outputs: ITransactionOutput[];
    lockTime: bigint;
    subnetworkId: HexString;
    gas: bigint;
    payload: HexString;
    /** The mass of the transaction (the mass is undefined or zero unless explicitly set or obtained from the node) */
    mass?: bigint;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionVerboseData;
}

/**
 * Optional transaction verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionVerboseData {
    transactionId : HexString;
    hash : HexString;
    computeMass : bigint;
    blockHash : HexString;
    blockTime : bigint;
}



/**
 * Interface defines the structure of a transaction input.
 * 
 * @category Consensus
 */
export interface ITransactionInput {
    previousOutpoint: ITransactionOutpoint;
    signatureScript?: HexString;
    sequence: bigint;
    sigOpCount: number;
    utxo?: UtxoEntryReference;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionInputVerboseData;
}

/**
 * Option transaction input verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionInputVerboseData { }




/**
 * Interface defines the structure of a transaction outpoint (used by transaction input).
 * 
 * @category Consensus
 */
export interface ITransactionOutpoint {
    transactionId: HexString;
    index: number;
}



/**
 * Interface defining the structure of a block header.
 * 
 * @category Consensus
 */
export interface IHeader {
    hash: HexString;
    version: number;
    parentsByLevel: Array<Array<HexString>>;
    hashMerkleRoot: HexString;
    acceptedIdMerkleRoot: HexString;
    utxoCommitment: HexString;
    timestamp: bigint;
    bits: number;
    nonce: bigint;
    daaScore: bigint;
    blueWork: bigint | HexString;
    blueScore: bigint;
    pruningPoint: HexString;
}

/**
 * Interface defining the structure of a raw block header.
 * 
 * This interface is explicitly used by GetBlockTemplate and SubmitBlock RPCs
 * and unlike `IHeader`, does not include a hash.
 * 
 * @category Consensus
 */
export interface IRawHeader {
    version: number;
    parentsByLevel: Array<Array<HexString>>;
    hashMerkleRoot: HexString;
    acceptedIdMerkleRoot: HexString;
    utxoCommitment: HexString;
    timestamp: bigint;
    bits: number;
    nonce: bigint;
    daaScore: bigint;
    blueWork: bigint | HexString;
    blueScore: bigint;
    pruningPoint: HexString;
}




/**
 * Interface defines the structure of a serializable UTXO entry.
 * 
 * @see {@link ISerializableTransactionInput}, {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableUtxoEntry {
    address?: Address;
    amount: bigint;
    scriptPublicKey: ScriptPublicKey;
    blockDaaScore: bigint;
    isCoinbase: boolean;
}

/**
 * Interface defines the structure of a serializable transaction input.
 * 
 * @see {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableTransactionInput {
    transactionId : HexString;
    index: number;
    sequence: bigint;
    sigOpCount: number;
    signatureScript?: HexString;
    utxo: ISerializableUtxoEntry;
}

/**
 * Interface defines the structure of a serializable transaction output.
 * 
 * @see {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableTransactionOutput {
    value: bigint;
    scriptPublicKey: IScriptPublicKey;
}

/**
 * Interface defines the structure of a serializable transaction.
 * 
 * Serializable transactions can be produced using 
 * {@link Transaction.serializeToJSON},
 * {@link Transaction.serializeToSafeJSON} and 
 * {@link Transaction.serializeToObject} 
 * functions for processing (signing) in external systems.
 * 
 * Once the transaction is signed, it can be deserialized
 * into {@link Transaction} using {@link Transaction.deserializeFromJSON}
 * and {@link Transaction.deserializeFromSafeJSON} functions. 
 * 
 * @see {@link Transaction},
 * {@link ISerializableTransactionInput},
 * {@link ISerializableTransactionOutput},
 * {@link ISerializableUtxoEntry}
 * 
 * @category Wallet SDK
 */
export interface ISerializableTransaction {
    id? : HexString;
    version: number;
    inputs: ISerializableTransactionInput[];
    outputs: ISerializableTransactionOutput[];
    lockTime: bigint;
    subnetworkId: HexString;
    gas: bigint;
    payload: HexString;
}




/**
 * Interface defines the structure of a Script Public Key.
 * 
 * @category Consensus
 */
export interface IScriptPublicKey {
    version : number;
    script: HexString;
}



/**
* Return interface for the {@link RpcClient.getFeeEstimateExperimental} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetFeeEstimateExperimentalResponse {
        estimate : IFeeEstimate;
        verbose? : IFeeEstimateVerboseExperimentalData
    }
    


/**
* Argument interface for the {@link RpcClient.getFeeEstimateExperimental} RPC method.
* Get fee estimate from the node.
*
* @category Node RPC
*/
    export interface IGetFeeEstimateExperimentalRequest { }
    


    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IFeeEstimateVerboseExperimentalData {
        mempoolReadyTransactionsCount : bigint;
        mempoolReadyTransactionsTotalMass : bigint;
        networkMassPerSecond : bigint;
        nextBlockTemplateFeerateMin : number;
        nextBlockTemplateFeerateMedian : number;
        nextBlockTemplateFeerateMax : number;
    }
    


/**
* Return interface for the {@link RpcClient.getFeeEstimate} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetFeeEstimateResponse {
        estimate : IFeeEstimate;
    }
    


/**
* Argument interface for the {@link RpcClient.getFeeEstimate} RPC method.
* Get fee estimate from the node.
*
* @category Node RPC
*/
    export interface IGetFeeEstimateRequest { }
    


    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IFeeEstimate {
        /**
         * *Top-priority* feerate bucket. Provides an estimation of the feerate required for sub-second DAG inclusion.
         *
         * Note: for all buckets, feerate values represent fee/mass of a transaction in `sompi/gram` units.
         * Given a feerate value recommendation, calculate the required fee by
         * taking the transaction mass and multiplying it by feerate: `fee = feerate * mass(tx)`
         */

        priorityBucket : IFeerateBucket;
        /**
         * A vector of *normal* priority feerate values. The first value of this vector is guaranteed to exist and
         * provide an estimation for sub-*minute* DAG inclusion. All other values will have shorter estimation
         * times than all `low_bucket` values. Therefor by chaining `[priority] | normal | low` and interpolating
         * between them, one can compose a complete feerate function on the client side. The API makes an effort
         * to sample enough "interesting" points on the feerate-to-time curve, so that the interpolation is meaningful.
         */

        normalBuckets : IFeerateBucket[];
        /**
        * An array of *low* priority feerate values. The first value of this vector is guaranteed to
        * exist and provide an estimation for sub-*hour* DAG inclusion.
        */
        lowBuckets : IFeerateBucket[];
    }
    


    /**
     * 
     * 
     * @category Node RPC
     */
    export interface IFeerateBucket {
        /**
         * The fee/mass ratio estimated to be required for inclusion time <= estimated_seconds
         */
        feerate : number;
        /**
         * The estimated inclusion time for a transaction with fee/mass = feerate
         */
        estimatedSeconds : number;
    }
    


/**
* Return interface for the {@link RpcClient.unban} RPC method.
*
*
* @category Node RPC
*/
    export interface IUnbanResponse { }
    


/**
* Argument interface for the {@link RpcClient.unban} RPC method.
*
*
* @category Node RPC
*/
    export interface IUnbanRequest {
/**
* IPv4 or IPv6 address to unban.
*/
        ip : string;
    }
    


/**
* Return interface for the {@link RpcClient.submitTransaction} RPC method.
*
*
* @category Node RPC
*/
    export interface ISubmitTransactionResponse {
        transactionId : HexString;
    }
    


/**
* Argument interface for the {@link RpcClient.submitTransaction} RPC method.
* Submit transaction to the node.
*
* @category Node RPC
*/
    export interface ISubmitTransactionRequest {
        transaction : Transaction,
        allowOrphan? : boolean
    }
    


/**
* Return interface for the {@link RpcClient.submitTransactionReplacement} RPC method.
*
*
* @category Node RPC
*/
    export interface ISubmitTransactionReplacementResponse {
        transactionId : HexString;
        replacedTransaction: Transaction;
    }
    


/**
* Argument interface for the {@link RpcClient.submitTransactionReplacement} RPC method.
* Submit transaction replacement to the node.
*
* @category Node RPC
*/
    export interface ISubmitTransactionReplacementRequest {
        transaction : Transaction,
    }
    


/**
* Return interface for the {@link RpcClient.submitBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface ISubmitBlockResponse {
        report : ISubmitBlockReport;
    }
    


    /**
     * 
     * @category Node RPC
     */
    export enum SubmitBlockRejectReason {
        /**
         * The block is invalid.
         */
        BlockInvalid = "BlockInvalid",
        /**
         * The node is not synced.
         */
        IsInIBD = "IsInIBD",
        /**
         * Route is full.
         */
        RouteIsFull = "RouteIsFull",
    }

    /**
     * 
     * @category Node RPC
     */
    export interface ISubmitBlockReport {
        type : "success" | "reject";
        reason? : SubmitBlockRejectReason;
    }



/**
* Argument interface for the {@link RpcClient.submitBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface ISubmitBlockRequest {
        block : IRawBlock;
        allowNonDAABlocks: boolean;
    }
    


/**
* Return interface for the {@link RpcClient.resolveFinalityConflict} RPC method.
*
*
* @category Node RPC
*/
    export interface IResolveFinalityConflictResponse { }
    


/**
* Argument interface for the {@link RpcClient.resolveFinalityConflict} RPC method.
*
*
* @category Node RPC
*/
    export interface IResolveFinalityConflictRequest {
        finalityBlockHash: HexString;
    }
    


/**
* Return interface for the {@link RpcClient.getVirtualChainFromBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetVirtualChainFromBlockResponse {
        removedChainBlockHashes : HexString[];
        addedChainBlockHashes : HexString[];
        acceptedTransactionIds : IAcceptedTransactionIds[];
    }
    


/**
* Argument interface for the {@link RpcClient.getVirtualChainFromBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetVirtualChainFromBlockRequest {
        startHash : HexString;
        includeAcceptedTransactionIds: boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getUtxosByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetUtxosByAddressesResponse {
        entries : UtxoEntryReference[];
    }
    


/**
* Argument interface for the {@link RpcClient.getUtxosByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetUtxosByAddressesRequest { 
        addresses : Address[] | string[]
    }
    


/**
* Return interface for the {@link RpcClient.getSubnetwork} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetSubnetworkResponse {
        gasLimit : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getSubnetwork} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetSubnetworkRequest {
        subnetworkId : HexString;
    }
    


/**
* Return interface for the {@link RpcClient.getMempoolEntry} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntryResponse {
        mempoolEntry : IMempoolEntry;
    }
    


/**
* Argument interface for the {@link RpcClient.getMempoolEntry} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntryRequest {
        transactionId : HexString;
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getMempoolEntriesByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntriesByAddressesResponse {
        entries : IMempoolEntry[];
    }
    


/**
* Argument interface for the {@link RpcClient.getMempoolEntriesByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntriesByAddressesRequest {
        addresses : Address[] | string[];
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getMempoolEntries} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntriesResponse {
        mempoolEntries : IMempoolEntry[];
    }
    


/**
* Argument interface for the {@link RpcClient.getMempoolEntries} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetMempoolEntriesRequest {
        includeOrphanPool? : boolean;
        filterTransactionPool? : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getHeaders} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetHeadersResponse {
        headers : IHeader[];
    }
    


/**
* Argument interface for the {@link RpcClient.getHeaders} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetHeadersRequest {
        startHash : HexString;
        limit : bigint;
        isAscending : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getCurrentNetwork} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetCurrentNetworkResponse {
        network : string;
    }
    


/**
* Argument interface for the {@link RpcClient.getCurrentNetwork} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetCurrentNetworkRequest { }
    


/**
* Return interface for the {@link RpcClient.getDaaScoreTimestampEstimate} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetDaaScoreTimestampEstimateResponse {
        timestamps : bigint[];
    }
    


/**
* Argument interface for the {@link RpcClient.getDaaScoreTimestampEstimate} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetDaaScoreTimestampEstimateRequest {
        daaScores : bigint[];
    }
    


/**
* Return interface for the {@link RpcClient.getCurrentBlockColor} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetCurrentBlockColorResponse {
        blue: boolean;
    }
    


/**
* Argument interface for the {@link RpcClient.getCurrentBlockColor} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetCurrentBlockColorRequest {
        hash: HexString;
    }
    


/**
* Return interface for the {@link RpcClient.getBlockTemplate} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlockTemplateResponse {
        block : IRawBlock;
    }
    


/**
* Argument interface for the {@link RpcClient.getBlockTemplate} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlockTemplateRequest {
        payAddress : Address | string;
/**
* `extraData` can contain a user-supplied plain text or a byte array represented by `Uint8array`.
*/
        extraData? : string | Uint8Array;
    }
    


/**
* Return interface for the {@link RpcClient.getBlocks} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlocksResponse {
        blockHashes : HexString[];
        blocks : IBlock[];
    }
    


/**
* Argument interface for the {@link RpcClient.getBlocks} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlocksRequest {
        lowHash? : HexString;
        includeBlocks : boolean;
        includeTransactions : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlockResponse {
        block : IBlock;
    }
    


/**
* Argument interface for the {@link RpcClient.getBlock} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBlockRequest {
        hash : HexString;
        includeTransactions : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getBalancesByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IBalancesByAddressesEntry {
        address : Address;
        balance : bigint;
    }
/**
*
*
* @category Node RPC
*/
    export interface IGetBalancesByAddressesResponse {
        entries : IBalancesByAddressesEntry[];
    }
    


/**
* Argument interface for the {@link RpcClient.getBalancesByAddresses} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBalancesByAddressesRequest {
        addresses : Address[] | string[];
    }
    


/**
* Return interface for the {@link RpcClient.getBalanceByAddress} RPC method.
*
*
* @category Node RPC
*/
    export interface IGetBalanceByAddressResponse {
        balance : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getBalanceByAddress} RPC method.
* @category Node RPC
*/
    export interface IGetBalanceByAddressRequest {
        address : Address | string;
    }
    


/**
* Return interface for the {@link RpcClient.estimateNetworkHashesPerSecond} RPC method.
* @category Node RPC
*/
    export interface IEstimateNetworkHashesPerSecondResponse {
        networkHashesPerSecond : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.estimateNetworkHashesPerSecond} RPC method.
* @category Node RPC
*/
    export interface IEstimateNetworkHashesPerSecondRequest {
        windowSize : number;
        startHash? : HexString;
    }
    


/**
* Return interface for the {@link RpcClient.ban} RPC method.
*
*
* @category Node RPC
*/
    export interface IBanResponse { }
    


/**
* Argument interface for the {@link RpcClient.ban} RPC method.
*
*
* @category Node RPC
*/
    export interface IBanRequest {
/**
* IPv4 or IPv6 address to ban.
*/
        ip : string;
    }
    


/**
* Return interface for the {@link RpcClient.addPeer} RPC method.
*
*
* @category Node RPC
*/
    export interface IAddPeerResponse { }
    


/**
* Argument interface for the {@link RpcClient.addPeer} RPC method.
*
*
* @category Node RPC
*/
    export interface IAddPeerRequest {
        peerAddress : INetworkAddress;
        isPermanent : boolean;
    }
    


/**
* Return interface for the {@link RpcClient.getSyncStatus} RPC method.
* @category Node RPC
*/
    export interface IGetSyncStatusResponse {
        isSynced : boolean;
    }
    


/**
* Argument interface for the {@link RpcClient.getSyncStatus} RPC method.
* @category Node RPC
*/
    export interface IGetSyncStatusRequest { }
    


/**
* Return interface for the {@link RpcClient.getServerInfo} RPC method.
* @category Node RPC
*/
    export interface IGetServerInfoResponse {
        rpcApiVersion : number[];
        serverVersion : string;
        networkId : string;
        hasUtxoIndex : boolean;
        isSynced : boolean;
        virtualDaaScore : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getServerInfo} RPC method.
* @category Node RPC
*/
    export interface IGetServerInfoRequest { }
    


/**
* Return interface for the {@link RpcClient.shutdown} RPC method.
* @category Node RPC
*/
    export interface IShutdownResponse { }
    


/**
* Argument interface for the {@link RpcClient.shutdown} RPC method.
* @category Node RPC
*/
    export interface IShutdownRequest { }
    


/**
* Return interface for the {@link RpcClient.getSinkBlueScore} RPC method.
* @category Node RPC
*/
    export interface IGetSinkBlueScoreResponse {
        blueScore : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getSinkBlueScore} RPC method.
* @category Node RPC
*/
    export interface IGetSinkBlueScoreRequest { }
    


/**
* Return interface for the {@link RpcClient.getSink} RPC method.
* @category Node RPC
*/
    export interface IGetSinkResponse {
        sink : HexString;
    }
    


/**
* Argument interface for the {@link RpcClient.getSink} RPC method.
* @category Node RPC
*/
    export interface IGetSinkRequest { }
    


/**
* Return interface for the {@link RpcClient.getConnections} RPC method.
* @category Node RPC
*/
    export interface IGetConnectionsResponse {
        [key: string]: any
    }
    


/**
* Argument interface for the {@link RpcClient.getConnections} RPC method.
* @category Node RPC
*/
    export interface IGetConnectionsRequest { }
    


/**
* Return interface for the {@link RpcClient.getMetrics} RPC method.
* @category Node RPC
*/
    export interface IGetMetricsResponse {
        [key: string]: any
    }
    


/**
* Argument interface for the {@link RpcClient.getMetrics} RPC method.
* @category Node RPC
*/
    export interface IGetMetricsRequest { }
    


/**
* Return interface for the {@link RpcClient.getPeerAddresses} RPC method.
* @category Node RPC
*/
    export interface IGetPeerAddressesResponse {
        [key: string]: any
    }
    


/**
* Argument interface for the {@link RpcClient.getPeerAddresses} RPC method.
* @category Node RPC
*/
    export interface IGetPeerAddressesRequest { }
    


/**
* Return interface for the {@link RpcClient.getInfo} RPC method.
* @category Node RPC
*/
    export interface IGetInfoResponse {
        p2pId : string;
        mempoolSize : bigint;
        serverVersion : string;
        isUtxoIndexed : boolean;
        isSynced : boolean;
/** GRPC ONLY */
        hasNotifyCommand : boolean;
/** GRPC ONLY */
        hasMessageId : boolean;
    }
    


/**
* Argument interface for the {@link RpcClient.getInfo} RPC method.
* @category Node RPC
*/
    export interface IGetInfoRequest { }
    


/**
* Return interface for the {@link RpcClient.getConnectedPeerInfo} RPC method.
* @category Node RPC
*/
    export interface IGetConnectedPeerInfoResponse {
        [key: string]: any
    }
    


/**
* Argument interface for the {@link RpcClient.getConnectedPeerInfo} RPC method.
* @category Node RPC
*/
    export interface IGetConnectedPeerInfoRequest { }
    


/**
* Return interface for the {@link RpcClient.getCoinSupply} RPC method.
* @category Node RPC
*/
    export interface IGetCoinSupplyResponse {
        maxSompi: bigint;
        circulatingSompi: bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getCoinSupply} RPC method.
* @category Node RPC
*/
    export interface IGetCoinSupplyRequest { }
    


/**
* Return interface for the {@link RpcClient.getBlockDagInfo} RPC method.
* @category Node RPC
*/
    export interface IGetBlockDagInfoResponse {
        network: string;
        blockCount: bigint;
        headerCount: bigint;
        tipHashes: HexString[];
        difficulty: number;
        pastMedianTime: bigint;
        virtualParentHashes: HexString[];
        pruningPointHash: HexString;
        virtualDaaScore: bigint;
        sink: HexString;
    }
    


/**
* Argument interface for the {@link RpcClient.getBlockDagInfo} RPC method.
* @category Node RPC
*/
    export interface IGetBlockDagInfoRequest { }
    


/**
* Return interface for the {@link RpcClient.getBlockCount} RPC method.
* @category Node RPC
*/
    export interface IGetBlockCountResponse {
        headerCount : bigint;
        blockCount : bigint;
    }
    


/**
* Argument interface for the {@link RpcClient.getBlockCount} RPC method.
* @category Node RPC
*/
    export interface IGetBlockCountRequest { }
    


/**
* Return interface for the {@link RpcClient.ping} RPC method.
* @category Node RPC
*/
    export interface IPingResponse {
        message?: string;
    }
    


/**
* Argument interface for the {@link RpcClient.ping} RPC method.
* @category Node RPC
*/
    export interface IPingRequest {
        message?: string;
    }
    


    /**
     * Accepted transaction IDs.
     * 
     * @category Node RPC
     */
    export interface IAcceptedTransactionIds {
        acceptingBlockHash : HexString;
        acceptedTransactionIds : HexString[];
    }



        /**
         * Interface defining the structure of a block.
         * 
         * @category Consensus
         */
        export interface IBlock {
            header: IHeader;
            transactions: ITransaction[];
            verboseData?: IBlockVerboseData;
        }

        /**
         * Interface defining the structure of a block verbose data.
         * 
         * @category Node RPC
         */
        export interface IBlockVerboseData {
            hash: HexString;
            difficulty: number;
            selectedParentHash: HexString;
            transactionIds: HexString[];
            isHeaderOnly: boolean;
            blueScore: number;
            childrenHashes: HexString[];
            mergeSetBluesHashes: HexString[];
            mergeSetRedsHashes: HexString[];
            isChainBlock: boolean;
        }

        /**
         * Interface defining the structure of a raw block.
         * 
         * Raw block is a structure used by GetBlockTemplate and SubmitBlock RPCs
         * and differs from `IBlock` in that it does not include verbose data and carries
         * `IRawHeader` that does not include a cached block hash.
         * 
         * @category Consensus
         */
        export interface IRawBlock {
            header: IRawHeader;
            transactions: ITransaction[];
        }

        


            /**
             * Mempool entry.
             * 
             * @category Node RPC
             */
            export interface IMempoolEntry {
                fee : bigint;
                transaction : ITransaction;
                isOrphan : boolean;
            }
        


    /**
     * Generic network address representation.
     * 
     * @category General
     */
    export interface INetworkAddress {
        /**
         * IPv4 or IPv6 address.
         */
        ip: string;
        /**
         * Optional port number.
         */
        port?: number;
    }



    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IAccountDescriptor {
        kind : AccountKind,
        accountId : HexString,
        accountName? : string,
        receiveAddress? : Address,
        changeAddress? : Address,
        prvKeyDataIds : HexString[],
        // balance? : Balance,
        [key: string]: any
    }
    



/**
 * Type of a binding record.
 * @see {@link IBinding}, {@link ITransactionDataVariant}, {@link ITransactionRecord}
 * @category Wallet SDK
 */
export enum BindingType {
    /**
     * The data structure is associated with a user-supplied id.
     * @see {@link IBinding}
     */
    Custom = "custom",
    /**
     * The data structure is associated with a wallet account.
     * @see {@link IBinding}, {@link Account}
     */
    Account = "account",
}

/**
 * Internal transaction data contained within the transaction record.
 * @see {@link ITransactionRecord}
 * @category Wallet SDK
 */
export interface IBinding {
    type : BindingType;
    data : HexString;
}



/**
 * 
 * 
 * @category Wallet SDK
 * 
 */
export enum TransactionKind {
    Reorg = "reorg",
    Stasis = "stasis",
    Batch = "batch",
    Change = "change",
    Incoming = "incoming",
    Outgoing = "outgoing",
    External = "external",
    TransferIncoming = "transfer-incoming",
    TransferOutgoing = "transfer-outgoing",
}



        interface Wallet {
            /**
            * @param {WalletNotificationCallback} callback
            */
            addEventListener(callback:WalletNotificationCallback): void;
            /**
            * @param {WalletEventType} event
            * @param {WalletNotificationCallback} [callback]
            */
            addEventListener<M extends keyof WalletEventMap>(
                event: M,
                callback: (eventData: WalletEventMap[M]) => void
            )
        }


    /**
     * 
     * 
     * @category  Wallet API
     */
    export interface IWalletConfig {
        /**
         * `resident` is a boolean indicating if the wallet should not be stored on the permanent medium.
         */
        resident?: boolean;
        networkId?: NetworkId | string;
        encoding?: Encoding | string;
        url?: string;
        resolver?: Resolver;
    }
    


        interface UtxoProcessor {
            /**
            * @param {UtxoProcessorNotificationCallback} callback
            */
            addEventListener(callback: UtxoProcessorNotificationCallback): void;
            /**
            * @param {UtxoProcessorEventType} event
            * @param {UtxoProcessorNotificationCallback} [callback]
            */
            addEventListener<E extends keyof UtxoProcessorEventMap>(
                event: E,
                callback: UtxoProcessorNotificationCallback<E>
            )
        }


    /**
     * UtxoProcessor constructor arguments.
     * 
     * @see {@link UtxoProcessor}, {@link UtxoContext}, {@link RpcClient}, {@link NetworkId}
     * @category Wallet SDK
     */
    export interface IUtxoProcessorArgs {
        /**
         * The RPC client to use for network communication.
         */
        rpc : RpcClient;
        networkId : NetworkId | string;
    }
    


/**
 * Configuration for the transaction {@link Generator}. This interface
 * allows you to specify UTXO sources, transaction outputs, change address,
 * priority fee, and other transaction parameters.
 * 
 * If the total number of UTXOs needed to satisfy the transaction outputs
 * exceeds maximum allowed number of UTXOs per transaction (limited by
 * the maximum transaction mass), the {@link Generator} will produce 
 * multiple chained transactions to the change address and then used these
 * transactions as a source for the "final" transaction.
 * 
 * @see 
 *      {@link kaspaToSompi},
 *      {@link Generator}, 
 *      {@link PendingTransaction}, 
 *      {@link UtxoContext}, 
 *      {@link UtxoEntry},
 *      {@link createTransactions},
 *      {@link estimateTransactions}
 * @category Wallet SDK
 */
interface IGeneratorSettingsObject {
    /** 
     * Final transaction outputs (do not supply change transaction).
     * 
     * Typical usage: { address: "kaspa:...", amount: 1000n }
     */
    outputs: PaymentOutput | IPaymentOutput[];
    /** 
     * Address to be used for change, if any. 
     */
    changeAddress: Address | string;
    /**
     * Fee rate in SOMPI per 1 gram of mass.
     * 
     * Fee rate is applied to all transactions generated by the {@link Generator}.
     * This includes batch and final transactions. If not set, the fee rate is 
     * not applied.
     */
    feeRate?: number;
    /** 
     * Priority fee in SOMPI.
     * 
     * If supplying `bigint` value, it will be interpreted as a sender-pays fee.
     * Alternatively you can supply an object with `amount` and `source` properties
     * where `source` contains the {@link FeeSource} enum.
     * 
     * **IMPORTANT:* When sending an outbound transaction (transaction that
     * contains outputs), the `priorityFee` must be set, even if it is zero.
     * However, if the transaction is missing outputs (and thus you are
     * creating a compound transaction against your change address),
     * `priorityFee` should not be set (i.e. it should be `undefined`).
     * 
     * @see {@link IFees}, {@link FeeSource}
     */
    priorityFee?: IFees | bigint;
    /**
     * UTXO entries to be used for the transaction. This can be an
     * array of UtxoEntry instances, objects matching {@link IUtxoEntry}
     * interface, or a {@link UtxoContext} instance.
     */
    entries: IUtxoEntry[] | UtxoEntryReference[] | UtxoContext;
    /**
     * Optional UTXO entries that will be consumed before those available in `entries`.
     * You can use this property to apply custom input selection logic.
     * Please note that these inputs are consumed first, then `entries` are consumed
     * to generate a desirable transaction output amount.  If transaction mass
     * overflows, these inputs will be consumed into a batch/sweep transaction
     * where the destination if the `changeAddress`.
     */
    priorityEntries?: IUtxoEntry[] | UtxoEntryReference[],
    /**
     * Optional number of signature operations in the transaction.
     */
    sigOpCount?: number;
    /**
     * Optional minimum number of signatures required for the transaction.
     */
    minimumSignatures?: number;
    /**
     * Optional data payload to be included in the transaction.
     */
    payload?: Uint8Array | HexString;

    /**
     * Optional NetworkId or network id as string (i.e. `mainnet` or `testnet-11`). Required when {@link IGeneratorSettingsObject.entries} is array
     */
    networkId?: NetworkId | string
}



    /**
     * UtxoContext constructor arguments.
     * 
     * @see {@link UtxoProcessor}, {@link UtxoContext}, {@link RpcClient}
     * @category Wallet SDK
     */
    export interface IUtxoContextArgs {
        /**
         * Associated UtxoProcessor.
         */
        processor: UtxoProcessor;
        /**
         * Optional id for the UtxoContext.
         * **The id must be a valid 32-byte hex string.**
         * You can use {@link sha256FromBinary} or {@link sha256FromText} to generate a valid id.
         * 
         * If not provided, a random id will be generated.
         * The IDs are deterministic, based on the order UtxoContexts are created.
         */
        id?: HexString;
    }
    





    /**
     * Interface defining response from the {@link createTransactions} function.
     * 
     * @category Wallet SDK
     */
    export interface ICreateTransactions {
        /**
         * Array of pending unsigned transactions.
         */
        transactions : PendingTransaction[];
        /**
         * Summary of the transaction generation process.
         */
        summary : GeneratorSummary;
    }
    


/**
 * 
 * Defines a single payment output.
 * 
 * @see {@link IGeneratorSettingsObject}, {@link Generator}
 * @category Wallet SDK
 */
export interface IPaymentOutput {
    /**
     * Destination address. The address prefix must match the network
     * you are transacting on (e.g. `kaspa:` for mainnet, `kaspatest:` for testnet, etc).
     */
    address: Address | string;
    /**
     * Output amount in SOMPI.
     */
    amount: bigint;
}



/**
 * {@link UtxoContext} (wallet account) balance.
 * @category Wallet SDK
 */
export interface IBalance {
    /**
     * Total amount of Kaspa (in SOMPI) available for 
     * spending.
     */
    mature: bigint;
    /**
     * Total amount of Kaspa (in SOMPI) that has been 
     * received and is pending confirmation.
     */
    pending: bigint;
    /**
     * Total amount of Kaspa (in SOMPI) currently 
     * being sent as a part of the outgoing transaction
     * but has not yet been accepted by the network.
     */
    outgoing: bigint;
    /**
     * Number of UTXOs available for spending.
     */
    matureUtxoCount: number;
    /**
     * Number of UTXOs that have been received and 
     * are pending confirmation.
     */
    pendingUtxoCount: number;
    /**
     * Number of UTXOs currently in stasis (coinbase 
     * transactions received as a result of mining).
     * Unlike regular user transactions, coinbase 
     * transactions go through `stasis->pending->mature`
     * stages. Client applications should ignore `stasis`
     * stages and should process transactions only when
     * they have reached the `pending` stage. However, 
     * `stasis` information can be used for informative 
     * purposes to indicate that coinbase transactions
     * have arrived.
     */
    stasisUtxoCount: number;
}



/**
* Return interface for the {@link Wallet.addressBookEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IAddressBookEnumerateResponse {
        // TODO
    }
    


/**
* Argument interface for the {@link Wallet.addressBookEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IAddressBookEnumerateRequest { }
    


/**
* Return interface for the {@link Wallet.transactionsReplaceMetadata} method.
*
*
* @category Wallet API
*/
    export interface ITransactionsReplaceMetadataResponse { }
    


/**
* Argument interface for the {@link Wallet.transactionsReplaceMetadata} method.
* Metadata is a wallet-specific string that can be used to store arbitrary data.
* It should contain a serialized JSON string with `key` containing the custom
* data stored by the wallet.  When interacting with metadata, the wallet should
* always deserialize the JSON string and then serialize it again after making
* changes, preserving any foreign keys that it might encounter.
*
* To preserve foreign metadata, the pattern of access should be:
* `Get -> Modify -> Replace`
*
* @category Wallet API
*/
    export interface ITransactionsReplaceMetadataRequest {
/**
* The id of account the transaction belongs to.
*/
        accountId: HexString,
/**
* The network id of the transaction.
*/
        networkId: NetworkId | string,
/**
* The id of the transaction.
*/
        transactionId: HexString,
/**
* Optional metadata string to replace the existing metadata.
* If not supplied, the metadata will be removed.
*/
        metadata?: string,    
    }
    


/**
* Return interface for the {@link Wallet.transactionsReplaceNote} method.
*
*
* @category Wallet API
*/
    export interface ITransactionsReplaceNoteResponse { }
    


/**
* Argument interface for the {@link Wallet.transactionsReplaceNote} method.
*
*
* @category Wallet API
*/
    export interface ITransactionsReplaceNoteRequest {
/**
* The id of account the transaction belongs to.
*/
        accountId: HexString,
/**
* The network id of the transaction.
*/
        networkId: NetworkId | string,
/**
* The id of the transaction.
*/
        transactionId: HexString,
/**
* Optional note string to replace the existing note.
* If not supplied, the note will be removed.
*/
        note?: string,
    }
    


/**
* Return interface for the {@link Wallet.transactionsDataGet} method.
*
*
* @category Wallet API
*/
    export interface ITransactionsDataGetResponse {
        accountId : HexString;
        transactions : ITransactionRecord[];
        start : bigint;
        total : bigint;
    }
    


/**
* Argument interface for the {@link Wallet.transactionsDataGet} method.
*
*
* @category Wallet API
*/
    export interface ITransactionsDataGetRequest {
        accountId : HexString;
        networkId : NetworkId | string;
        filter? : TransactionKind[];
        start : bigint;
        end : bigint;
    }
    


/**
* Return interface for the {@link Wallet.accountsEstimate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEstimateResponse {
        generatorSummary : GeneratorSummary;
    }
    


/**
* Argument interface for the {@link Wallet.accountsEstimate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEstimateRequest {
        accountId : HexString;
        destination : IPaymentOutput[];
        feeRate? : number;
        priorityFeeSompi : IFees | bigint;
        payload? : Uint8Array | string;
    }
    


/**
* Return interface for the {@link Wallet.accountsTransfer} method.
*
*
* @category Wallet API
*/
    export interface IAccountsTransferResponse {
        generatorSummary : GeneratorSummary;
        transactionIds : HexString[];
    }
    


/**
* Argument interface for the {@link Wallet.accountsTransfer} method.
*
*
* @category Wallet API
*/
    export interface IAccountsTransferRequest {
        sourceAccountId : HexString;
        destinationAccountId : HexString;
        walletSecret : string;
        paymentSecret? : string;
        feeRate? : number;
        priorityFeeSompi? : IFees | bigint;
        transferAmountSompi : bigint;
    }
    


/**
* Return interface for the {@link Wallet.accountsSend} method.
*
*
* @category Wallet API
*/
    export interface IAccountsSendResponse {
/**
* Summary produced by the transaction generator.
*/
        generatorSummary : GeneratorSummary;
/**
* Hex identifiers of successfully submitted transactions.
*/
        transactionIds : HexString[];
    }
    


/**
* Argument interface for the {@link Wallet.accountsSend} method.
*
*
* @category Wallet API
*/
    export interface IAccountsSendRequest {
/**
* Hex identifier of the account.
*/
        accountId : HexString;
/**
* Wallet encryption secret.
*/
        walletSecret : string;
/**
* Optional key encryption secret or BIP39 passphrase.
*/
        paymentSecret? : string;
/**
* Fee rate in sompi per 1 gram of mass.
*/
        feeRate? : number;
/**
* Priority fee.
*/
        priorityFeeSompi? : IFees | bigint;
/**
*
*/
        payload? : Uint8Array | HexString;
/**
* If not supplied, the destination will be the change address resulting in a UTXO compound transaction.
*/
        destination? : IPaymentOutput[];
    }
    


/**
* Return interface for the {@link Wallet.accountsCreateNewAddress} method.
*
*
* @category Wallet API
*/
    export interface IAccountsCreateNewAddressResponse {
        address: Address;
    }
    


/**
* Argument interface for the {@link Wallet.accountsCreateNewAddress} method.
*
*
* @category Wallet API
*/
    export interface IAccountsCreateNewAddressRequest {
        accountId: string;
        addressKind?: NewAddressKind | string,
    }
    


/**
* Return interface for the {@link Wallet.accountsGet} method.
*
*
* @category Wallet API
*/
    export interface IAccountsGetResponse {
        accountDescriptor: IAccountDescriptor;
    }
    


/**
* Argument interface for the {@link Wallet.accountsGet} method.
*
*
* @category Wallet API
*/
    export interface IAccountsGetRequest {
        accountId: string;
    }
    


/**
* Return interface for the {@link Wallet.accountsDeactivate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsDeactivateResponse { }
    


/**
* Argument interface for the {@link Wallet.accountsDeactivate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsDeactivateRequest {
        accountIds?: string[];
    }
    


/**
* Return interface for the {@link Wallet.accountsActivate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsActivateResponse { }
    


/**
* Argument interface for the {@link Wallet.accountsActivate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsActivateRequest {
        accountIds?: HexString[],
    }
    


/**
* Return interface for the {@link Wallet.accountsImport} method.
*
*
* @category Wallet API
*/
    export interface IAccountsImportResponse {
        // TODO
    }
    


/**
* Argument interface for the {@link Wallet.accountsImport} method.
*
*
* @category Wallet API
*/
    export interface IAccountsImportRequest {
        walletSecret: string;
        // TODO
    }
    


/**
* Return interface for the {@link Wallet.accountsEnsureDefault} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEnsureDefaultResponse {
        accountDescriptor : IAccountDescriptor;
    }
    


/**
* Argument interface for the {@link Wallet.accountsEnsureDefault} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEnsureDefaultRequest {
        walletSecret: string;
        paymentSecret?: string;
        type : AccountKind | string;
        mnemonic? : string;
    }
    


/**
* Return interface for the {@link Wallet.accountsCreate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsCreateResponse {
        accountDescriptor : IAccountDescriptor;
    }
    


/**
* Argument interface for the {@link Wallet.accountsCreate} method.
*
*
* @category Wallet API
*/
    export type IAccountsCreateRequest = {
        walletSecret: string;
        type: "bip32";
        accountName:string;
        accountIndex?:number;
        prvKeyDataId:string;
        paymentSecret?:string;
    };
    //   |{
    //     walletSecret: string;
    //     type: "multisig";
    //     accountName:string;
    //     accountIndex?:number;
    //     prvKeyDataId:string;
    //     pubkeys:HexString[];
    //     paymentSecret?:string;
    //   }

    //   |{
    //     walletSecret: string;
    //     type: "bip32-readonly";
    //     accountName:string;
    //     accountIndex?:number;
    //     pubkey:HexString;
    //     paymentSecret?:string;
    //  }
    


/**
* Return interface for the {@link Wallet.accountsDiscovery} method.
*
*
* @category Wallet API
*/
    export interface IAccountsDiscoveryResponse {
        lastAccountIndexFound : number;
    }
    


/**
* Argument interface for the {@link Wallet.accountsDiscovery} method.
*
*
* @category Wallet API
*/
    export interface IAccountsDiscoveryRequest {
        discoveryKind: AccountsDiscoveryKind,
        accountScanExtent: number,
        addressScanExtent: number,
        bip39_passphrase?: string,
        bip39_mnemonic: string,
    }
    


/**
* Return interface for the {@link Wallet.accountsRename} method.
*
*
* @category Wallet API
*/
    export interface IAccountsRenameResponse { }
    


/**
* Argument interface for the {@link Wallet.accountsRename} method.
*
*
* @category Wallet API
*/
    export interface IAccountsRenameRequest {
        accountId: string;
        name?: string;
        walletSecret: string;
    }
    


/**
* Return interface for the {@link Wallet.accountsEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEnumerateResponse {
        accountDescriptors: IAccountDescriptor[];
    }
    


/**
* Argument interface for the {@link Wallet.accountsEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IAccountsEnumerateRequest { }
    


/**
* Return interface for the {@link Wallet.prvKeyDataGet} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataGetResponse {
        // prvKeyData: PrvKeyData,
    }
    


/**
* Argument interface for the {@link Wallet.prvKeyDataGet} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataGetRequest {
        walletSecret: string;
        prvKeyDataId: HexString;
    }
    


/**
* Return interface for the {@link Wallet.prvKeyDataRemove} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataRemoveResponse { }
    


/**
* Argument interface for the {@link Wallet.prvKeyDataRemove} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataRemoveRequest {
        walletSecret: string;
        prvKeyDataId: HexString;
    }
    


/**
* Return interface for the {@link Wallet.prvKeyDataCreate} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataCreateResponse {
        prvKeyDataId: HexString;
    }
    


/**
* Argument interface for the {@link Wallet.prvKeyDataCreate} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataCreateRequest {
/** Wallet encryption secret */
        walletSecret: string;
/** Optional name of the private key */
        name? : string;
/**
* Optional key secret (BIP39 passphrase).
*
* If supplied, all operations requiring access
* to the key will require the `paymentSecret`
* to be provided.
*/
        paymentSecret? : string;
/** BIP39 mnemonic phrase (12 or 24 words)*/
        mnemonic : string;
    }
    


/**
* Return interface for the {@link Wallet.prvKeyDataEnumerate} method.
*
* Response returning a list of private key ids, their optional names and properties.
*
* @see {@link IPrvKeyDataInfo}
* @category Wallet API
*/
    export interface IPrvKeyDataEnumerateResponse {
        prvKeyDataList: IPrvKeyDataInfo[],
    }
    


/**
* Argument interface for the {@link Wallet.prvKeyDataEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IPrvKeyDataEnumerateRequest { }
    


/**
* Return interface for the {@link Wallet.walletImport} method.
*
*
* @category Wallet API
*/
    export interface IWalletImportResponse { }
    


/**
* Argument interface for the {@link Wallet.walletImport} method.
*
*
* @category Wallet API
*/
    export interface IWalletImportRequest {
        walletSecret: string;
        walletData: HexString | Uint8Array;
    }
    


/**
* Return interface for the {@link Wallet.walletExport} method.
*
*
* @category Wallet API
*/
    export interface IWalletExportResponse {
        walletData: HexString;
    }
    


/**
* Argument interface for the {@link Wallet.walletExport} method.
*
*
* @category Wallet API
*/
    export interface IWalletExportRequest {
        walletSecret: string;
        includeTransactions: boolean;
    }
    


/**
* Return interface for the {@link Wallet.walletChangeSecret} method.
*
*
* @category Wallet API
*/
    export interface IWalletChangeSecretResponse { }
    


/**
* Argument interface for the {@link Wallet.walletChangeSecret} method.
*
*
* @category Wallet API
*/
    export interface IWalletChangeSecretRequest {
        oldWalletSecret: string;
        newWalletSecret: string;
    }
    


/**
* Return interface for the {@link Wallet.walletReload} method.
*
*
* @category Wallet API
*/
    export interface IWalletReloadResponse { }
    


/**
* Argument interface for the {@link Wallet.walletReload} method.
*
*
* @category Wallet API
*/
    export interface IWalletReloadRequest {
/**
* Reactivate accounts that are active before the reload.
*/
        reactivate: boolean;
    }
    


/**
* Return interface for the {@link Wallet.walletClose} method.
*
*
* @category Wallet API
*/
    export interface IWalletCloseResponse { }
    


/**
* Argument interface for the {@link Wallet.walletClose} method.
*
*
* @category Wallet API
*/
    export interface IWalletCloseRequest { }
    


/**
* Return interface for the {@link Wallet.walletOpen} method.
*
*
* @category Wallet API
*/
    export interface IWalletOpenResponse {
        accountDescriptors: IAccountDescriptor[];
    }
    


/**
* Argument interface for the {@link Wallet.walletOpen} method.
*
* @category Wallet API
*/
    export interface IWalletOpenRequest {
        walletSecret: string;
        filename?: string;
        accountDescriptors: boolean;
    }
    


/**
* Return interface for the {@link Wallet.walletCreate} method.
*
*
* @category Wallet API
*/
    export interface IWalletCreateResponse {
        walletDescriptor: IWalletDescriptor;
        storageDescriptor: IStorageDescriptor;
    }
    


/**
* Argument interface for the {@link Wallet.walletCreate} method.
*
* If filename is not supplied, the filename will be derived from the wallet title.
* If both wallet title and filename are not supplied, the wallet will be create
* with the default filename `kaspa`.
*
* @category Wallet API
*/
    export interface IWalletCreateRequest {
/** Wallet encryption secret */
        walletSecret: string;
/** Optional wallet title */
        title?: string;
/** Optional wallet filename */
        filename?: string;
/** Optional user hint */
        userHint?: string;
/**
* Overwrite wallet data if the wallet with the same filename already exists.
* (Use with caution!)
*/
        overwriteWalletStorage?: boolean;
    }
    


/**
* Return interface for the {@link Wallet.walletEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IWalletEnumerateResponse {
        walletDescriptors: WalletDescriptor[];
    }
    


/**
* Argument interface for the {@link Wallet.walletEnumerate} method.
*
*
* @category Wallet API
*/
    export interface IWalletEnumerateRequest { }
    


/**
* Return interface for the {@link Wallet.retainContext} method.
*
*
* @category Wallet API
*/
    export interface IRetainContextResponse {
    }
    


/**
* Argument interface for the {@link Wallet.retainContext} method.
*
*
* @category Wallet API
*/
    export interface IRetainContextRequest {
/**
* Optional context creation name.
*/
        name : string;
/**
* Optional context data to retain.
*/
        data? : string;
    }
    


/**
* Return interface for the {@link Wallet.getStatus} method.
*
*
* @category Wallet API
*/
    export interface IGetStatusResponse {
        isConnected : boolean;
        isSynced : boolean;
        isOpen : boolean;
        url? : string;
        networkId? : NetworkId;
        context? : HexString;
    }
    


/**
* Argument interface for the {@link Wallet.getStatus} method.
*
*
* @category Wallet API
*/
    export interface IGetStatusRequest {
/**
* Optional context creation name.
* @see {@link IRetainContextRequest}
*/
        name? : string;
    }
    


/**
* Return interface for the {@link Wallet.disconnect} method.
*
*
* @category Wallet API
*/
    export interface IDisconnectResponse { }
    


/**
* Argument interface for the {@link Wallet.disconnect} method.
*
*
* @category Wallet API
*/
    export interface IDisconnectRequest { }
    


/**
* Return interface for the {@link Wallet.connect} method.
*
*
* @category Wallet API
*/
    export interface IConnectResponse { }
    


/**
* Argument interface for the {@link Wallet.connect} method.
*
*
* @category Wallet API
*/
    export interface IConnectRequest {
        // destination wRPC node URL (if omitted, the resolver is used)
        url? : string;
        // network identifier
        networkId : NetworkId | string;
        // retry on error
        retryOnError? : boolean;
        // block async connect (method will not return until the connection is established)
        block? : boolean;
        // require node to be synced (fail otherwise)
        requireSync? : boolean;
    }
    


/**
* Return interface for the {@link Wallet.flush} method.
*
*
* @category Wallet API
*/
    export interface IFlushResponse { }
    


/**
* Argument interface for the {@link Wallet.flush} method.
*
*
* @category Wallet API
*/
    export interface IFlushRequest {
        walletSecret : string;
    }
    


/**
* Return interface for the {@link Wallet.batch} method.
*
*
* @category Wallet API
*/
    export interface IBatchResponse { }
    


/**
* Argument interface for the {@link Wallet.batch} method.
* Suspend storage operations until invocation of flush().
*
* @category Wallet API
*/
    export interface IBatchRequest { }
    


/**
 * @categoryDescription Wallet API
 * Wallet API for interfacing with Rusty Kaspa Wallet implementation.
 */



    /**
     * Emitted by {@link UtxoProcessor} when node is syncing cryptographic trust data as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncTrustSyncEvent {
        processed : number;
        total : number;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when node is syncing the UTXO set as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncUtxoSyncEvent {
        chunks : number;
        total : number;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when node is syncing blocks as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncBlocksEvent {
        blocks : number;
        progress : number;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when node is syncing headers as a part of the IBD (Initial Block Download) process.
     * 
     * @category Wallet Events
     */
    export interface ISyncHeadersEvent {
        headers : number;
        progress : number;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when node is syncing and processing cryptographic proofs.
     * 
     * @category Wallet Events
     */
    export interface ISyncProofEvent {
        level : number;
    }
    


    /**
     * Emitted when detecting a general error condition.
     * 
     * @category Wallet Events
     */
    export interface IErrorEvent {
        message : string;
    }
    


    /**
     * Emitted by {@link UtxoContext} when detecting a balance change.
     * This notification is produced during the UTXO scan, when UtxoContext
     * detects incoming or outgoing transactions or when transactions
     * change their state (e.g. from pending to confirmed).
     * 
     * @category Wallet Events
     */
    export interface IBalanceEvent {
        id : HexString;
        balance? : IBalance;
    }
    


    /**
     * Emitted by {@link UtxoContext} when detecting a new transaction during
     * the initialization phase. Discovery transactions indicate that UTXOs
     * have been discovered during the initial UTXO scan.
     * 
     * When receiving such notifications, the application should check its 
     * internal storage to see if the transaction already exists. If it doesn't,
     * it should create a correspond in record and notify the user of a new
     * transaction.
     * 
     * This event is emitted when an address has existing UTXO entries that
     * may have been received during previous sessions or while the wallet
     * was offline.
     * 
     * @category Wallet Events
     */
    export type IDiscoveryEvent = TransactionRecord;
    


    /**
     * Emitted by {@link UtxoContext} when transaction is considered to be confirmed.
     * This notification will be followed by the "balance" event.
     * 
     * @category Wallet Events
     */
    export type IMaturityEvent = TransactionRecord;
    


    /**
     * Emitted by {@link UtxoContext} when detecting a new coinbase transaction.
     * Transactions are kept in "stasis" for the half of the coinbase maturity DAA period.
     * A wallet should ignore these transactions until they are re-broadcasted
     * via the "pending" event.
     * 
     * @category Wallet Events
     */
    export type IStasisEvent = TransactionRecord;
    


    /**
     * Emitted by {@link UtxoContext} when detecting a reorg transaction condition.
     * A transaction is considered reorg if it has been removed from the UTXO set
     * as a part of the network reorg process. Transactions notified with this event
     * should be considered as invalid and should be removed from the application state.
     * Associated UTXOs will be automatically removed from the UtxoContext state.
     * 
     * @category Wallet Events
     */
    export type IReorgEvent = TransactionRecord;
    


    /**
     * Emitted by {@link UtxoContext} when detecting a pending transaction.
     * This notification will be followed by the "balance" event.
     * 
     * @category Wallet Events
     */
    export type IPendingEvent = TransactionRecord;
    


    /**
     * Emitted by {@link UtxoProcessor} on DAA score change.
     * 
     * @category Wallet Events
     */
    export interface IDaaScoreChangeEvent {
        currentDaaScore : number;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} indicating a non-recoverable internal error.
     * If such event is emitted, the application should stop the UtxoProcessor
     * and restart all related subsystem. This event is emitted when the UtxoProcessor
     * encounters a critical condition such as "out of memory".
     * 
     * @category Wallet Events
     */
    export interface IUtxoProcErrorEvent {
        message : string;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} after successfully opening an RPC
     * connection to the Kaspa node. This event contains general information
     * about the Kaspa node.
     * 
     * @category Wallet Events
     */
    export interface IServerStatusEvent {
        networkId : string;
        serverVersion : string;
        isSynced : boolean;
        url? : string;
    }
    


    /**
     * Emitted by {@link Wallet} when an account data has been updated.
     * This event signifies a chance in the internal account state that
     * includes new address generation.
     * 
     * @category Wallet Events
     */
    export interface IAccountUpdateEvent {
        accountDescriptor : IAccountDescriptor;
    }
    


    /**
     * Emitted by {@link Wallet} when an account has been created.
     * 
     * @category Wallet Events
     */
    export interface IAccountCreateEvent {
        accountDescriptor : IAccountDescriptor;
    }
    


    /**
     * Emitted by {@link Wallet} when an account has been selected.
     * This event is used internally in Rust SDK to track currently
     * selected account in the Rust CLI wallet.
     * 
     * @category Wallet Events
     */
    export interface IAccountSelectionEvent {
        id? : HexString;
    }
    


    /**
     * Emitted by {@link Wallet} when an account has been deactivated.
     * 
     * @category Wallet Events
     */
    export interface IAccountDeactivationEvent {
        ids : HexString[];
    }
    


    /**
     * Emitted by {@link Wallet} when an account has been activated.
     * 
     * @category Wallet Events
     */
    export interface IAccountActivationEvent {
        ids : HexString[];
    }
    


    /**
     * Emitted by {@link Wallet} when the wallet has created a private key.
     * 
     * @category Wallet Events
     */
    export interface IPrvKeyDataCreateEvent {
        prvKeyDataInfo : IPrvKeyDataInfo;
    }
    


    /**
     * Emitted by {@link Wallet} when an error occurs (for example, the wallet has failed to open).
     * 
     * @category Wallet Events
     */
    export interface IWalletErrorEvent {
        message : string;
    }
    


    /**
     * Emitted by {@link Wallet} when the wallet is successfully reloaded.
     * 
     * @category Wallet Events
     */
    export interface IWalletReloadEvent {
        walletDescriptor : IWalletDescriptor;
        accountDescriptors : IAccountDescriptor[];
    }
    


    /**
     * Emitted by {@link Wallet} when the wallet data storage has been successfully created.
     * 
     * @category Wallet Events
     */
    export interface IWalletCreateEvent {
        walletDescriptor : IWalletDescriptor;
        storageDescriptor : IStorageDescriptor;
    }
    


    /**
     * Emitted by {@link Wallet} when the wallet is successfully opened.
     * 
     * @category Wallet Events
     */
    export interface IWalletOpenEvent {
        walletDescriptor : IWalletDescriptor;
        accountDescriptors : IAccountDescriptor[];
    }
    


    /**
     * Emitted by {@link Wallet} when it opens and contains an optional anti-phishing 'hint' set by the user.
     * 
     * @category Wallet Events
     */
    export interface IWalletHintEvent {
        hint? : string;
    }
    



    /**
     * 
     * @category Wallet Events
     */
    export interface ISyncState {
        event : string;
        data? : ISyncProofEvent | ISyncHeadersEvent | ISyncBlocksEvent | ISyncUtxoSyncEvent | ISyncTrustSyncEvent;
    }
    
    /**
     * 
     * @category Wallet Events
     */
    export interface ISyncStateEvent {
        syncState : ISyncState;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when it detects that connected node does not have UTXO index enabled.
     * 
     * @category Wallet Events
     */
    export interface IUtxoIndexNotEnabledEvent {
        url? : string;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when it disconnects from RPC.
     * 
     * @category Wallet Events
     */
    export interface IDisconnectEvent {
        networkId : string;
        url? : string;
    }
    


    /**
     * Emitted by {@link UtxoProcessor} when it negotiates a successful RPC connection.
     * 
     * @category Wallet Events
     */
    export interface IConnectEvent {
        networkId : string;
        url? : string;
    }
    



        /**
         * Events emitted by the {@link Wallet}.
         * @category Wallet API
         */
        export enum WalletEventType {
            Connect = "connect",
            Disconnect = "disconnect",
            UtxoIndexNotEnabled = "utxo-index-not-enabled",
            SyncState = "sync-state",
            WalletHint = "wallet-hint",
            WalletOpen = "wallet-open",
            WalletCreate = "wallet-create",
            WalletReload = "wallet-reload",
            WalletError = "wallet-error",
            WalletClose = "wallet-close",
            PrvKeyDataCreate = "prv-key-data-create",
            AccountActivation = "account-activation",
            AccountDeactivation = "account-deactivation",
            AccountSelection = "account-selection",
            AccountCreate = "account-create",
            AccountUpdate = "account-update",
            ServerStatus = "server-status",
            UtxoProcStart = "utxo-proc-start",
            UtxoProcStop = "utxo-proc-stop",
            UtxoProcError = "utxo-proc-error",
            DaaScoreChange = "daa-score-change",
            Pending = "pending",
            Reorg = "reorg",
            Stasis = "stasis",
            Maturity = "maturity",
            Discovery = "discovery",
            Balance = "balance",
            Error = "error",
        }

        /**
         * Wallet notification event data map.
         * @see {@link Wallet.addEventListener}
         * @category Wallet API
         */
        export type WalletEventMap = {
            "connect": IConnectEvent,
            "disconnect": IDisconnectEvent,
            "utxo-index-not-enabled": IUtxoIndexNotEnabledEvent,
            "sync-state": ISyncStateEvent,
            "wallet-hint": IWalletHintEvent,
            "wallet-open": IWalletOpenEvent,
            "wallet-create": IWalletCreateEvent,
            "wallet-reload": IWalletReloadEvent,
            "wallet-error": IWalletErrorEvent,
            "wallet-close": undefined,
            "prv-key-data-create": IPrvKeyDataCreateEvent,
            "account-activation": IAccountActivationEvent,
            "account-deactivation": IAccountDeactivationEvent,
            "account-selection": IAccountSelectionEvent,
            "account-create": IAccountCreateEvent,
            "account-update": IAccountUpdateEvent,
            "server-status": IServerStatusEvent,
            "utxo-proc-start": undefined,
            "utxo-proc-stop": undefined,
            "utxo-proc-error": IUtxoProcErrorEvent,
            "daa-score-change": IDaaScoreChangeEvent,
            "pending": IPendingEvent,
            "reorg": IReorgEvent,
            "stasis": IStasisEvent,
            "maturity": IMaturityEvent,
            "discovery": IDiscoveryEvent,
            "balance": IBalanceEvent,
            "error": IErrorEvent,
        }
        
        /**
         * {@link Wallet} notification event interface.
         * @category Wallet API
         */
        export type IWalletEvent<T extends keyof WalletEventMap> = {
            [K in T]: {
                type: K,
                data: WalletEventMap[K]
            }
        }[T];


        /**
         * Wallet notification callback type.
         * 
         * This type declares the callback function that is called when notification is emitted
         * from the Wallet (and the underlying UtxoProcessor or UtxoContext subsystems).
         * 
         * @see {@link Wallet}
         * 
         * @category Wallet API
         */
        export type WalletNotificationCallback<E extends keyof WalletEventMap = keyof WalletEventMap> = (event: IWalletEvent<E>) => void;
        



        /**
         * Events emitted by the {@link UtxoProcessor}.
         * @category Wallet SDK
         */
        export enum UtxoProcessorEventType {
            Connect = "connect",
            Disconnect = "disconnect",
            UtxoIndexNotEnabled = "utxo-index-not-enabled",
            SyncState = "sync-state",
            UtxoProcStart = "utxo-proc-start",
            UtxoProcStop = "utxo-proc-stop",
            UtxoProcError = "utxo-proc-error",
            DaaScoreChange = "daa-score-change",
            Pending = "pending",
            Reorg = "reorg",
            Stasis = "stasis",
            Maturity = "maturity",
            Discovery = "discovery",
            Balance = "balance",
            Error = "error",
        }

        /**
         * {@link UtxoProcessor} notification event data map.
         * 
         * @category Wallet API
         */
        export type UtxoProcessorEventMap = {
            "connect": IConnectEvent,
            "disconnect": IDisconnectEvent,
            "utxo-index-not-enabled": IUtxoIndexNotEnabledEvent,
            "sync-state": ISyncStateEvent,
            "server-status": IServerStatusEvent,
            "utxo-proc-start": undefined,
            "utxo-proc-stop": undefined,
            "utxo-proc-error": IUtxoProcErrorEvent,
            "daa-score-change": IDaaScoreChangeEvent,
            "pending": IPendingEvent,
            "reorg": IReorgEvent,
            "stasis": IStasisEvent,
            "maturity": IMaturityEvent,
            "discovery": IDiscoveryEvent,
            "balance": IBalanceEvent,
            "error": IErrorEvent
        }

        /**
         * 
         * @category Wallet API
         */

        export type UtxoProcessorEvent<T extends keyof UtxoProcessorEventMap> = {
          [K in T]: {
            type: K,
            data: UtxoProcessorEventMap[K]
          }
        }[T];
        
        /**
         * {@link UtxoProcessor} notification callback type.
         * 
         * This type declares the callback function that is called when notification is emitted
         * from the UtxoProcessor or UtxoContext subsystems.
         * 
         * @see {@link UtxoProcessor}, {@link UtxoContext},
         * 
         * @category Wallet SDK
         */

        export type UtxoProcessorNotificationCallback<E extends keyof UtxoProcessorEventMap = keyof UtxoProcessorEventMap> = (event: UtxoProcessorEvent<E>) => void;
        


/**
 * Interface declaration for {@link verifyMessage} function arguments.
 * 
 * @category Message Signing
 */
export interface IVerifyMessage {
    message: string;
    signature: HexString;
    publicKey: PublicKey | string;
}



/**
 * Interface declaration for {@link signMessage} function arguments.
 * 
 * @category Message Signing
 */
export interface ISignMessage {
    message: string;
    privateKey: PrivateKey | string;
}




export interface IPrvKeyDataArgs {
    prvKeyDataId: HexString;
    paymentSecret?: string;
}

export interface IAccountCreateArgsBip32 {
    accountName?: string;
    accountIndex?: number;
}

/**
 * @category Wallet API
 */
export interface IAccountCreateArgs {
    type : "bip32";
    args : IAccountCreateArgsBip32;
    prvKeyDataArgs? : IPrvKeyDataArgs;
}



    /**
     * 
     * @category Wallet SDK
     */
    export interface IFees {
        amount: bigint;
        source?: FeeSource;
    }
    


/**
 * Wallet storage information.
 * 
 * @category Wallet API
 */
export interface IWalletDescriptor {
    title?: string;
    filename: string;
}



/**
 * Wallet storage information.
 */
export interface IStorageDescriptor {
    kind: string;
    data: string;
}



    /**
     * Private key data information.
     * @category Wallet API
     */
    export interface IPrvKeyDataInfo {
        /** Deterministic wallet id of the private key */
        id: HexString;
        /** Optional name of the private key */
        name?: string;
        /** 
         * Indicates if the key requires additional payment or a recovery secret
         * to perform wallet operations that require access to it.
         * For BIP39 keys this indicates that the key was created with a BIP39 passphrase.
         */
        isEncrypted: boolean;
    }
    



/**
 * 
 * @category Wallet SDK
 */
export interface IUtxoRecord {
    address?: Address;
    index: number;
    amount: bigint;
    scriptPublicKey: HexString;
    isCoinbase: boolean;
}

/**
 * Type of transaction data record.
 * @see {@link ITransactionData}, {@link ITransactionDataVariant}, {@link ITransactionRecord}
 * @category Wallet SDK
 */
export enum TransactionDataType {
    /**
     * Transaction has been invalidated due to a BlockDAG reorganization.
     * Such transaction is no longer valid and its UTXO entries are removed.
     * @see {@link ITransactionDataReorg}
     */
    Reorg = "reorg",
    /**
     * Transaction has been received and its UTXO entries are added to the 
     * pending or mature UTXO set.
     * @see {@link ITransactionDataIncoming}
     */
    Incoming = "incoming",
    /**
     * Transaction is in stasis and its UTXO entries are not yet added to the UTXO set.
     * This event is generated for **Coinbase** transactions only.
     * @see {@link ITransactionDataStasis}
     */
    Stasis = "stasis",
    /**
     * Observed transaction is not performed by the wallet subsystem but is executed
     * against the address set managed by the wallet subsystem.
     * @see {@link ITransactionDataExternal}
     */
    External = "external",
    /**
     * Transaction is outgoing and its UTXO entries are removed from the UTXO set.
     * @see {@link ITransactionDataOutgoing}
     */
    Outgoing = "outgoing",
    /**
     * Transaction is a batch transaction (compounding UTXOs to an internal change address).
     * @see {@link ITransactionDataBatch}
     */
    Batch = "batch",
    /**
     * Transaction is an incoming transfer from another {@link UtxoContext} managed by the {@link UtxoProcessor}.
     * When operating under the integrated wallet, these are transfers between different wallet accounts.
     * @see {@link ITransactionDataTransferIncoming}
     */
    TransferIncoming = "transfer-incoming",
    /**
     * Transaction is an outgoing transfer to another {@link UtxoContext} managed by the {@link UtxoProcessor}.
     * When operating under the integrated wallet, these are transfers between different wallet accounts.
     * @see {@link ITransactionDataTransferOutgoing}
     */
    TransferOutgoing = "transfer-outgoing",
    /**
     * Transaction is a change transaction and its UTXO entries are added to the UTXO set.
     * @see {@link ITransactionDataChange}
     */
    Change = "change",
}

/**
 * Contains UTXO entries and value for a transaction
 * that has been invalidated due to a BlockDAG reorganization.
 * @category Wallet SDK
 */
export interface ITransactionDataReorg {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for an incoming transaction.
 * @category Wallet SDK
 */
export interface ITransactionDataIncoming {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for a stasis transaction.
 * @category Wallet SDK
 */
export interface ITransactionDataStasis {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Contains UTXO entries and value for an external transaction.
 * An external transaction is a transaction that was not issued 
 * by this instance of the wallet but belongs to this address set.
 * @category Wallet SDK
 */
export interface ITransactionDataExternal {
    utxoEntries: IUtxoRecord[];
    value: bigint;
}

/**
 * Batch transaction data (created by the {@link Generator} as a 
 * result of UTXO compounding process).
 * @category Wallet SDK
 */
export interface ITransactionDataBatch {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Outgoing transaction data.
 * @category Wallet SDK
 */
export interface ITransactionDataOutgoing {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Incoming transfer transaction data.
 * Transfer occurs when a transaction is issued between 
 * two {@link UtxoContext} (wallet account) instances.
 * @category Wallet SDK
 */
export interface ITransactionDataTransferIncoming {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Outgoing transfer transaction data.
 * Transfer occurs when a transaction is issued between 
 * two {@link UtxoContext} (wallet account) instances.
 * @category Wallet SDK
 */
export interface ITransactionDataTransferOutgoing {
    fees: bigint;
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Change transaction data.
 * @category Wallet SDK
 */
export interface ITransactionDataChange {
    inputValue: bigint;
    outputValue: bigint;
    transaction: ITransaction;
    paymentValue: bigint;
    changeValue: bigint;
    acceptedDaaScore?: bigint;
    utxoEntries: IUtxoRecord[];
}

/**
 * Transaction record data variants.
 * @category Wallet SDK
 */
export type ITransactionDataVariant = 
    ITransactionDataReorg
    | ITransactionDataIncoming
    | ITransactionDataStasis
    | ITransactionDataExternal
    | ITransactionDataOutgoing
    | ITransactionDataBatch
    | ITransactionDataTransferIncoming
    | ITransactionDataTransferOutgoing
    | ITransactionDataChange;

/**
 * Internal transaction data contained within the transaction record.
 * @see {@link ITransactionRecord}
 * @category Wallet SDK
 */
export interface ITransactionData {
    type : TransactionDataType;
    data : ITransactionDataVariant;
}

/**
 * Transaction record generated by the Kaspa Wallet SDK.
 * This data structure is delivered within {@link UtxoProcessor} and `Wallet` notification events.
 * @see {@link ITransactionData}, {@link TransactionDataType}, {@link ITransactionDataVariant}
 * @category Wallet SDK
 */
export interface ITransactionRecord {
    /**
     * Transaction id.
     */
    id: string;
    /**
     * Transaction UNIX time in milliseconds.
     */
    unixtimeMsec?: bigint;
    /**
     * Transaction value in SOMPI.
     */
    value: bigint;
    /**
     * Transaction binding (id of UtxoContext or Wallet Account).
     */
    binding: HexString;
    /**
     * Block DAA score.
     */
    blockDaaScore: bigint;
    /**
     * Network id on which this transaction has occurred.
     */
    network: NetworkId;
    /**
     * Transaction data.
     */
    data: ITransactionData;
    /**
     * Optional transaction note as a human-readable string.
     */
    note?: string;
    /**
     * Optional transaction metadata.
     * 
     * If present, this must contain a JSON-serialized string.
     * A client application updating the metadata must deserialize
     * the string into JSON, add a key with it's own identifier
     * and store its own metadata into the value of this key.
     */
    metadata?: string;

    /**
     * Transaction data type.
     */
    type: string;
}



/**
 * A string containing a hexadecimal representation of the data (typically representing for IDs or Hashes).
 * 
 * @category General
 */ 
export type HexString = string;



/**
 * Color range configuration for Hex View.
 * 
 * @category General
 */ 
export interface IHexViewColor {
    start: number;
    end: number;
    color?: string;
    background?: string;
}

/**
 * Configuration interface for Hex View.
 * 
 * @category General
 */ 
export interface IHexViewConfig {
    offset? : number;
    replacementCharacter? : string;
    width? : number;
    colors? : IHexViewColor[];
}



    /**
     * RPC Resolver connection options
     * 
     * @category Node RPC
     */
    export interface IResolverConnect {
        /**
         * RPC encoding: `borsh` (default) or `json`
         */
        encoding?: Encoding | string;
        /**
         * Network identifier: `mainnet` or `testnet-11` etc.
         */
        networkId?: NetworkId | string;
    }
    


    /**
     * RPC Resolver configuration options
     * 
     * @category Node RPC
     */
    export interface IResolverConfig {
        /**
         * Optional URLs for one or multiple resolvers.
         */
        urls?: string[];
        /**
         * Use strict TLS for RPC connections.
         * If not set or `false` (default), the resolver will
         * provide the best available connection regardless of
         * whether this connection supports TLS or not.
         * If set to `true`, the resolver will only provide
         * TLS-enabled connections.
         * 
         * This setting is ignored in the browser environment
         * when the browser navigator location is `https`.
         * In which case the resolver will always use TLS-enabled
         * connections.
         */
        tls?: boolean;
    }
    


    /**
     * New block template notification event is produced when a new block
     * template is generated for mining in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface INewBlockTemplate {
        [key: string]: any;
    }
    


    /**
     * Pruning point UTXO set override notification event is produced when the
     * UTXO set override for the pruning point changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IPruningPointUtxoSetOverride {
        [key: string]: any;
    }
    


    /**
     * Virtual DAA score changed notification event is produced when the virtual
     * Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualDaaScoreChanged {
        [key: string]: any;
    }
    


    /**
     * Sink blue score changed notification event is produced when the blue
     * score of the sink block changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface ISinkBlueScoreChanged {
        [key: string]: any;
    }
    


    /**
     * UTXOs changed notification event is produced when the set
     * of unspent transaction outputs (UTXOs) changes in the
     * Kaspa BlockDAG. The event notification is scoped to the
     * monitored list of addresses specified during the subscription.
     * 
     * @category Node RPC
     */
    export interface IUtxosChanged {
        [key: string]: any;
    }
    


    /**
     * Finality conflict resolved notification event is produced when a finality
     * conflict in the Kaspa BlockDAG is resolved.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflictResolved {
        [key: string]: any;
    }
    


    /**
     * Finality conflict notification event is produced when a finality
     * conflict occurs in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IFinalityConflict {
        [key: string]: any;
    }
    


    /**
     * Virtual chain changed notification event is produced when the virtual
     * chain changes in the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IVirtualChainChanged {
        [key: string]: any;
    }
    


    /**
     * Block added notification event is produced when a new
     * block is added to the Kaspa BlockDAG.
     * 
     * @category Node RPC
     */
    export interface IBlockAdded {
        [key: string]: any;
    }
    



/**
 * RPC notification events.
 * 
 * @see {RpcClient.addEventListener}, {RpcClient.removeEventListener}
 */
export enum RpcEventType {
    Connect = "connect",
    Disconnect = "disconnect",
    BlockAdded = "block-added",
    VirtualChainChanged = "virtual-chain-changed",
    FinalityConflict = "finality-conflict",
    FinalityConflictResolved = "finality-conflict-resolved",
    UtxosChanged = "utxos-changed",
    SinkBlueScoreChanged = "sink-blue-score-changed",
    VirtualDaaScoreChanged = "virtual-daa-score-changed",
    PruningPointUtxoSetOverride = "pruning-point-utxo-set-override",
    NewBlockTemplate = "new-block-template",
}

/**
 * RPC notification data payload.
 * 
 * @category Node RPC
 */
export type RpcEventData = IBlockAdded 
    | IVirtualChainChanged 
    | IFinalityConflict 
    | IFinalityConflictResolved 
    | IUtxosChanged 
    | ISinkBlueScoreChanged 
    | IVirtualDaaScoreChanged 
    | IPruningPointUtxoSetOverride 
    | INewBlockTemplate;

/**
 * RPC notification event data map.
 * 
 * @category Node RPC
 */
export type RpcEventMap = {
    "connect" : undefined,
    "disconnect" : undefined,
    "block-added" : IBlockAdded,
    "virtual-chain-changed" : IVirtualChainChanged,
    "finality-conflict" : IFinalityConflict,
    "finality-conflict-resolved" : IFinalityConflictResolved,
    "utxos-changed" : IUtxosChanged,
    "sink-blue-score-changed" : ISinkBlueScoreChanged,
    "virtual-daa-score-changed" : IVirtualDaaScoreChanged,
    "pruning-point-utxo-set-override" : IPruningPointUtxoSetOverride,
    "new-block-template" : INewBlockTemplate,
}

/**
 * RPC notification event.
 * 
 * @category Node RPC
 */
export type RpcEvent = {
    [K in keyof RpcEventMap]: { event: K, data: RpcEventMap[K] }
}[keyof RpcEventMap];

/**
 * RPC notification callback type.
 * 
 * This type is used to define the callback function that is called when an RPC notification is received.
 * 
 * @see {@link RpcClient.subscribeVirtualDaaScoreChanged},
 * {@link RpcClient.subscribeUtxosChanged}, 
 * {@link RpcClient.subscribeVirtualChainChanged},
 * {@link RpcClient.subscribeBlockAdded},
 * {@link RpcClient.subscribeFinalityConflict},
 * {@link RpcClient.subscribeFinalityConflictResolved},
 * {@link RpcClient.subscribeSinkBlueScoreChanged},
 * {@link RpcClient.subscribePruningPointUtxoSetOverride},
 * {@link RpcClient.subscribeNewBlockTemplate},
 * 
 * @category Node RPC
 */
export type RpcEventCallback = (event: RpcEvent) => void;




        interface RpcClient {
            /**
            * @param {RpcEventCallback} callback
            */
            addEventListener(callback:RpcEventCallback): void;
            /**
            * @param {RpcEventType} event
            * @param {RpcEventCallback} [callback]
            */
            addEventListener<M extends keyof RpcEventMap>(
                event: M,
                callback: (eventData: RpcEventMap[M]) => void
            )
        }


    /**
     * RPC client configuration options
     * 
     * @category Node RPC
     */
    export interface IRpcConfig {
        /**
         * An instance of the {@link Resolver} class to use for an automatic public node lookup.
         * If supplying a resolver, the `url` property is ignored.
         */
        resolver? : Resolver,
        /**
         * URL for wRPC node endpoint
         */
        url?: string;
        /**
         * RPC encoding: `borsh` or `json` (default is `borsh`)
         */
        encoding?: Encoding;
        /**
         * Network identifier: `mainnet`, `testnet-10` etc.
         * `networkId` is required when using a resolver.
         */
        networkId?: NetworkId | string;
    }
    


/**
 * Interface for configuring workflow-rs WASM32 bindings.
 * 
 * @category General
 */
export interface IWASM32BindingsConfig {
    /**
     * This option can be used to disable the validation of class names
     * for instances of classes exported by Rust WASM32 when passing
     * these classes to WASM32 functions.
     * 
     * This can be useful to programmatically disable checks when using
     * a bundler that mangles class symbol names.
     */
    validateClassNames : boolean;
}




        /**
         * `ConnectOptions` is used to configure the `WebSocket` connectivity behavior.
         * 
         * @category WebSocket
         */
        export interface IConnectOptions {
            /**
             * Indicates if the `async fn connect()` method should return immediately
             * or wait for connection to occur or fail before returning.
             * (default is `true`)
             */
            blockAsyncConnect? : boolean,
            /**
             * ConnectStrategy used to configure the retry or fallback behavior.
             * In retry mode, the WebSocket will continuously attempt to connect to the server.
             * (default is {link ConnectStrategy.Retry}).
             */
            strategy?: ConnectStrategy | string,
            /** 
             * A custom URL that will change the current URL of the WebSocket.
             * If supplied, the URL will override the use of resolver.
             */
            url?: string,
            /**
             * A custom connection timeout in milliseconds.
             */
            timeoutDuration?: number,
            /** 
             * A custom retry interval in milliseconds.
             */
            retryInterval?: number,
        }
        



        /**
         * `WebSocketConfig` is used to configure the `WebSocket`.
         * 
         * @category WebSocket
         */
        export interface IWebSocketConfig {
            /** Maximum size of the WebSocket message. */
            maxMessageSize: number,
            /** Maximum size of the WebSocket frame. */
            maxFrameSize: number,
        }
        

/**
*
* Abortable trigger wraps an `Arc<AtomicBool>`, which can be cloned
* to signal task terminating using an atomic bool.
*
* ```text
* let abortable = Abortable::default();
* let result = my_task(abortable).await?;
* // ... elsewhere
* abortable.abort();
* ```
*
* @category General
*/
export class Abortable {
  free(): void;
/**
*/
  constructor();
/**
* @returns {boolean}
*/
  isAborted(): boolean;
/**
*/
  abort(): void;
/**
*/
  check(): void;
/**
*/
  reset(): void;
}
/**
* Error emitted by [`Abortable`].
* @category General
*/
export class Aborted {
  free(): void;
}
/**
* @category Wallet SDK
*/
export class AccountKind {
  free(): void;
/**
* @param {string} kind
*/
  constructor(kind: string);
/**
* @returns {string}
*/
  toString(): string;
}
/**
* Kaspa `Address` struct that serializes to and from an address format string: `kaspa:qz0s...t8cv`.
* @category Address
*/
export class Address {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {string} address
*/
  constructor(address: string);
/**
* @param {string} address
* @returns {boolean}
*/
  static validate(address: string): boolean;
/**
* Convert an address to a string.
* @returns {string}
*/
  toString(): string;
/**
* @param {number} n
* @returns {string}
*/
  short(n: number): string;
/**
*/
  readonly payload: string;
/**
*/
  readonly prefix: string;
/**
*/
  setPrefix: string;
/**
*/
  readonly version: string;
}
/**
*/
export class AgentConstructorOptions {
  free(): void;
/**
*/
  keep_alive: boolean;
/**
*/
  keep_alive_msecs: number;
/**
*/
  max_free_sockets: number;
/**
*/
  max_sockets: number;
/**
*/
  timeout: number;
}
/**
*/
export class AppendFileOptions {
  free(): void;
/**
* @param {string | undefined} [encoding]
* @param {number | undefined} [mode]
* @param {string | undefined} [flag]
*/
  constructor(encoding?: string, mode?: number, flag?: string);
/**
* @returns {AppendFileOptions}
*/
  static new(): AppendFileOptions;
/**
*/
  encoding?: string;
/**
*/
  flag?: string;
/**
*/
  mode?: number;
}
/**
*/
export class AssertionErrorOptions {
  free(): void;
/**
* @param {string | undefined} message
* @param {any} actual
* @param {any} expected
* @param {string} operator
*/
  constructor(message: string | undefined, actual: any, expected: any, operator: string);
/**
* The actual property on the error instance.
*/
  actual: any;
/**
* The expected property on the error instance.
*/
  expected: any;
/**
* If provided, the error message is set to this value.
*/
  message?: string;
/**
* The operator property on the error instance.
*/
  operator: string;
}
/**
*
* Represents a {@link UtxoContext} (account) balance.
*
* @see {@link IBalance}, {@link UtxoContext}
*
* @category Wallet SDK
*/
export class Balance {
  free(): void;
/**
* @param {NetworkType | NetworkId | string} network_type
* @returns {BalanceStrings}
*/
  toBalanceStrings(network_type: NetworkType | NetworkId | string): BalanceStrings;
/**
* Confirmed amount of funds available for spending.
*/
  readonly mature: bigint;
/**
* Amount of funds that are being send and are not yet accepted by the network.
*/
  readonly outgoing: bigint;
/**
* Amount of funds that are being received and are not yet confirmed.
*/
  readonly pending: bigint;
}
/**
*
* Formatted string representation of the {@link Balance}.
*
* The value is formatted as `123,456.789`.
*
* @category Wallet SDK
*/
export class BalanceStrings {
  free(): void;
/**
*/
  readonly mature: string;
/**
*/
  readonly pending: string | undefined;
}
/**
*/
export class ConsoleConstructorOptions {
  free(): void;
/**
* @param {any} stdout
* @param {any} stderr
* @param {boolean | undefined} ignore_errors
* @param {any} color_mod
* @param {object | undefined} [inspect_options]
*/
  constructor(stdout: any, stderr: any, ignore_errors: boolean | undefined, color_mod: any, inspect_options?: object);
/**
* @param {any} stdout
* @param {any} stderr
* @returns {ConsoleConstructorOptions}
*/
  static new(stdout: any, stderr: any): ConsoleConstructorOptions;
/**
*/
  color_mod: any;
/**
*/
  ignore_errors?: boolean;
/**
*/
  inspect_options?: object;
/**
*/
  stderr: any;
/**
*/
  stdout: any;
}
/**
*/
export class CreateHookCallbacks {
  free(): void;
/**
* @param {Function} init
* @param {Function} before
* @param {Function} after
* @param {Function} destroy
* @param {Function} promise_resolve
*/
  constructor(init: Function, before: Function, after: Function, destroy: Function, promise_resolve: Function);
/**
*/
  after: Function;
/**
*/
  before: Function;
/**
*/
  destroy: Function;
/**
*/
  init: Function;
/**
*/
  promise_resolve: Function;
}
/**
*/
export class CreateReadStreamOptions {
  free(): void;
/**
* @param {boolean | undefined} [auto_close]
* @param {boolean | undefined} [emit_close]
* @param {string | undefined} [encoding]
* @param {number | undefined} [end]
* @param {number | undefined} [fd]
* @param {string | undefined} [flags]
* @param {number | undefined} [high_water_mark]
* @param {number | undefined} [mode]
* @param {number | undefined} [start]
*/
  constructor(auto_close?: boolean, emit_close?: boolean, encoding?: string, end?: number, fd?: number, flags?: string, high_water_mark?: number, mode?: number, start?: number);
/**
*/
  auto_close?: boolean;
/**
*/
  emit_close?: boolean;
/**
*/
  encoding?: string;
/**
*/
  end?: number;
/**
*/
  fd?: number;
/**
*/
  flags?: string;
/**
*/
  high_water_mark?: number;
/**
*/
  mode?: number;
/**
*/
  start?: number;
}
/**
*/
export class CreateWriteStreamOptions {
  free(): void;
/**
* @param {boolean | undefined} [auto_close]
* @param {boolean | undefined} [emit_close]
* @param {string | undefined} [encoding]
* @param {number | undefined} [fd]
* @param {string | undefined} [flags]
* @param {number | undefined} [mode]
* @param {number | undefined} [start]
*/
  constructor(auto_close?: boolean, emit_close?: boolean, encoding?: string, fd?: number, flags?: string, mode?: number, start?: number);
/**
*/
  auto_close?: boolean;
/**
*/
  emit_close?: boolean;
/**
*/
  encoding?: string;
/**
*/
  fd?: number;
/**
*/
  flags?: string;
/**
*/
  mode?: number;
/**
*/
  start?: number;
}
/**
*
* CryptoBox allows for encrypting and decrypting messages using the `crypto_box` crate.
*
* https://docs.rs/crypto_box/0.9.1/crypto_box/
*
*  @category Wallet SDK
*/
export class CryptoBox {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {CryptoBoxPrivateKey | HexString | Uint8Array} secretKey
* @param {CryptoBoxPublicKey | HexString | Uint8Array} peerPublicKey
*/
  constructor(secretKey: CryptoBoxPrivateKey | HexString | Uint8Array, peerPublicKey: CryptoBoxPublicKey | HexString | Uint8Array);
/**
* @param {string} plaintext
* @returns {string}
*/
  encrypt(plaintext: string): string;
/**
* @param {string} base64string
* @returns {string}
*/
  decrypt(base64string: string): string;
/**
*/
  readonly publicKey: string;
}
/**
* @category Wallet SDK
*/
export class CryptoBoxPrivateKey {
  free(): void;
/**
* @param {HexString | Uint8Array} secretKey
*/
  constructor(secretKey: HexString | Uint8Array);
/**
* @returns {CryptoBoxPublicKey}
*/
  to_public_key(): CryptoBoxPublicKey;
}
/**
* @category Wallet SDK
*/
export class CryptoBoxPublicKey {
  free(): void;
/**
* @param {HexString | Uint8Array} publicKey
*/
  constructor(publicKey: HexString | Uint8Array);
/**
* @returns {string}
*/
  toString(): string;
}
/**
* Key derivation path
* @category Wallet SDK
*/
export class DerivationPath {
  free(): void;
/**
* @param {string} path
*/
  constructor(path: string);
/**
* Is this derivation path empty? (i.e. the root)
* @returns {boolean}
*/
  isEmpty(): boolean;
/**
* Get the count of [`ChildNumber`] values in this derivation path.
* @returns {number}
*/
  length(): number;
/**
* Get the parent [`DerivationPath`] for the current one.
*
* Returns `Undefined` if this is already the root path.
* @returns {DerivationPath | undefined}
*/
  parent(): DerivationPath | undefined;
/**
* Push a [`ChildNumber`] onto an existing derivation path.
* @param {number} child_number
* @param {boolean | undefined} [hardened]
*/
  push(child_number: number, hardened?: boolean): void;
/**
* @returns {string}
*/
  toString(): string;
}
/**
*/
export class FormatInputPathObject {
  free(): void;
/**
* @param {string | undefined} [base]
* @param {string | undefined} [dir]
* @param {string | undefined} [ext]
* @param {string | undefined} [name]
* @param {string | undefined} [root]
*/
  constructor(base?: string, dir?: string, ext?: string, name?: string, root?: string);
/**
* @returns {FormatInputPathObject}
*/
  static new(): FormatInputPathObject;
/**
*/
  base?: string;
/**
*/
  dir?: string;
/**
*/
  ext?: string;
/**
*/
  name?: string;
/**
*/
  root?: string;
}
/**
* Generator is a type capable of generating transactions based on a supplied
* set of UTXO entries or a UTXO entry producer (such as {@link UtxoContext}). The Generator
* accumulates UTXO entries until it can generate a transaction that meets the
* requested amount or until the total mass of created inputs exceeds the allowed
* transaction mass, at which point it will produce a compound transaction by forwarding
* all selected UTXO entries to the supplied change address and prepare to start generating
* a new transaction.  Such sequence of daisy-chained transactions is known as a "batch".
* Each compound transaction results in a new UTXO, which is immediately reused in the
* subsequent transaction.
*
* The Generator constructor accepts a single {@link IGeneratorSettingsObject} object.
*
* ```javascript
*
* let generator = new Generator({
*     utxoEntries : [...],
*     changeAddress : "kaspa:...",
*     outputs : [
*         { amount : kaspaToSompi(10.0), address: "kaspa:..."},
*         { amount : kaspaToSompi(20.0), address: "kaspa:..."},
*         ...
*     ],
*     priorityFee : 1000n,
* });
*
* let pendingTransaction;
* while(pendingTransaction = await generator.next()) {
*     await pendingTransaction.sign(privateKeys);
*     await pendingTransaction.submit(rpc);
* }
*
* let summary = generator.summary();
* console.log(summary);
*
* ```
* @see
*     {@link IGeneratorSettingsObject},
*     {@link PendingTransaction},
*     {@link UtxoContext},
*     {@link createTransactions},
*     {@link estimateTransactions},
* @category Wallet SDK
*/
export class Generator {
  free(): void;
/**
* @param {IGeneratorSettingsObject} args
*/
  constructor(args: IGeneratorSettingsObject);
/**
* Generate next transaction
* @returns {Promise<any>}
*/
  next(): Promise<any>;
/**
* @returns {Promise<GeneratorSummary>}
*/
  estimate(): Promise<GeneratorSummary>;
/**
* @returns {GeneratorSummary}
*/
  summary(): GeneratorSummary;
}
/**
*
* A class containing a summary produced by transaction {@link Generator}.
* This class contains the number of transactions, the aggregated fees,
* the aggregated UTXOs and the final transaction amount that includes
* both network and QoS (priority) fees.
*
* @see {@link createTransactions}, {@link IGeneratorSettingsObject}, {@link Generator}
* @category Wallet SDK
*/
export class GeneratorSummary {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  readonly fees: bigint;
/**
*/
  readonly finalAmount: bigint | undefined;
/**
*/
  readonly finalTransactionId: string | undefined;
/**
*/
  readonly mass: bigint;
/**
*/
  readonly networkType: NetworkType;
/**
*/
  readonly transactions: number;
/**
*/
  readonly utxos: number;
}
/**
*/
export class GetNameOptions {
  free(): void;
/**
* @param {number | undefined} family
* @param {string} host
* @param {string} local_address
* @param {number} port
* @returns {GetNameOptions}
*/
  static new(family: number | undefined, host: string, local_address: string, port: number): GetNameOptions;
/**
*/
  family?: number;
/**
*/
  host: string;
/**
*/
  local_address: string;
/**
*/
  port: number;
}
/**
* @category General
*/
export class Hash {
  free(): void;
/**
* @param {string} hex_str
*/
  constructor(hex_str: string);
/**
* @returns {string}
*/
  toString(): string;
}
/**
* @category Consensus
*/
export class Header {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {Header | IHeader | IRawHeader} js_value
*/
  constructor(js_value: Header | IHeader | IRawHeader);
/**
* Finalizes the header and recomputes (updates) the header hash
* @return { String } header hash
* @returns {string}
*/
  finalize(): string;
/**
* Obtain `JSON` representation of the header. JSON representation
* should be obtained using WASM, to ensure proper serialization of
* big integers.
* @returns {string}
*/
  asJSON(): string;
/**
* @returns {string}
*/
  getBlueWorkAsHex(): string;
/**
*/
  acceptedIdMerkleRoot: any;
/**
*/
  bits: number;
/**
*/
  blueScore: bigint;
/**
*/
  blueWork: any;
/**
*/
  daaScore: bigint;
/**
*/
  readonly hash: string;
/**
*/
  hashMerkleRoot: any;
/**
*/
  nonce: bigint;
/**
*/
  parentsByLevel: any;
/**
*/
  pruningPoint: any;
/**
*/
  timestamp: bigint;
/**
*/
  utxoCommitment: any;
/**
*/
  version: number;
}
/**
* Data structure that contains a secret and public keys.
* @category Wallet SDK
*/
export class Keypair {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Get the [`Address`] of this Keypair's [`PublicKey`].
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddress(network: NetworkType | NetworkId | string): Address;
/**
* Get `ECDSA` [`Address`] of this Keypair's [`PublicKey`].
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = keypair.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddressECDSA(network: NetworkType | NetworkId | string): Address;
/**
* Create a new random [`Keypair`].
* JavaScript: `let keypair = Keypair::random();`.
* @returns {Keypair}
*/
  static random(): Keypair;
/**
* Create a new [`Keypair`] from a [`PrivateKey`].
* JavaScript: `let privkey = new PrivateKey(hexString); let keypair = privkey.toKeypair();`.
* @param {PrivateKey} secret_key
* @returns {Keypair}
*/
  static fromPrivateKey(secret_key: PrivateKey): Keypair;
/**
* Get the [`PrivateKey`] of this [`Keypair`].
*/
  readonly privateKey: string;
/**
* Get the [`PublicKey`] of this [`Keypair`].
*/
  readonly publicKey: string;
/**
* Get the `XOnlyPublicKey` of this [`Keypair`].
*/
  readonly xOnlyPublicKey: any;
}
/**
*/
export class MkdtempSyncOptions {
  free(): void;
/**
* @param {string | undefined} [encoding]
*/
  constructor(encoding?: string);
/**
* @returns {MkdtempSyncOptions}
*/
  static new(): MkdtempSyncOptions;
/**
*/
  encoding?: string;
}
/**
* BIP39 mnemonic phrases: sequences of words representing cryptographic keys.
* @category Wallet SDK
*/
export class Mnemonic {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {string} phrase
* @param {Language | undefined} [language]
*/
  constructor(phrase: string, language?: Language);
/**
* Validate mnemonic phrase. Returns `true` if the phrase is valid, `false` otherwise.
* @param {string} phrase
* @param {Language | undefined} [language]
* @returns {boolean}
*/
  static validate(phrase: string, language?: Language): boolean;
/**
* @param {number | undefined} [word_count]
* @returns {Mnemonic}
*/
  static random(word_count?: number): Mnemonic;
/**
* @param {string | undefined} [password]
* @returns {string}
*/
  toSeed(password?: string): string;
/**
*/
  entropy: string;
/**
*/
  phrase: string;
}
/**
*/
export class NetServerOptions {
  free(): void;
/**
*/
  allow_half_open?: boolean;
/**
*/
  pause_on_connect?: boolean;
}
/**
*
* NetworkId is a unique identifier for a kaspa network instance.
* It is composed of a network type and an optional suffix.
*
* @category Consensus
*/
export class NetworkId {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {any} value
*/
  constructor(value: any);
/**
* @returns {string}
*/
  toString(): string;
/**
* @returns {string}
*/
  addressPrefix(): string;
/**
*/
  readonly id: string;
/**
*/
  suffix?: number;
/**
*/
  type: NetworkType;
}
/**
*
* Data structure representing a Node connection endpoint
* as provided by the {@link Resolver}.
*
* @category Node RPC
*/
export class NodeDescriptor {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* The unique identifier of the node.
*/
  uid: string;
/**
* The URL of the node WebSocket (wRPC URL).
*/
  url: string;
}
/**
*/
export class PSKT {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {PSKT | Transaction | string | undefined} payload
*/
  constructor(payload: PSKT | Transaction | string | undefined);
/**
* Change role to `CREATOR`
* #[wasm_bindgen(js_name = toCreator)]
* @returns {PSKT}
*/
  creator(): PSKT;
/**
* Change role to `CONSTRUCTOR`
* @returns {PSKT}
*/
  toConstructor(): PSKT;
/**
* Change role to `UPDATER`
* @returns {PSKT}
*/
  toUpdater(): PSKT;
/**
* Change role to `SIGNER`
* @returns {PSKT}
*/
  toSigner(): PSKT;
/**
* Change role to `COMBINER`
* @returns {PSKT}
*/
  toCombiner(): PSKT;
/**
* Change role to `FINALIZER`
* @returns {PSKT}
*/
  toFinalizer(): PSKT;
/**
* Change role to `EXTRACTOR`
* @returns {PSKT}
*/
  toExtractor(): PSKT;
/**
* @param {bigint} lock_time
* @returns {PSKT}
*/
  fallbackLockTime(lock_time: bigint): PSKT;
/**
* @returns {PSKT}
*/
  inputsModifiable(): PSKT;
/**
* @returns {PSKT}
*/
  outputsModifiable(): PSKT;
/**
* @returns {PSKT}
*/
  noMoreInputs(): PSKT;
/**
* @returns {PSKT}
*/
  noMoreOutputs(): PSKT;
/**
* @param {ITransactionInput | TransactionInput} input
* @returns {PSKT}
*/
  input(input: ITransactionInput | TransactionInput): PSKT;
/**
* @param {ITransactionOutput | TransactionOutput} output
* @returns {PSKT}
*/
  output(output: ITransactionOutput | TransactionOutput): PSKT;
/**
* @param {bigint} n
* @param {number} input_index
* @returns {PSKT}
*/
  setSequence(n: bigint, input_index: number): PSKT;
/**
* @returns {Hash}
*/
  calculateId(): Hash;
/**
*/
  readonly payload: any;
/**
*/
  readonly role: string;
}
/**
* @category Wallet SDK
*/
export class PaymentOutput {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {Address} address
* @param {bigint} amount
*/
  constructor(address: Address, amount: bigint);
/**
*/
  address: Address;
/**
*/
  amount: bigint;
}
/**
* @category Wallet SDK
*/
export class PaymentOutputs {
  free(): void;
/**
* @param {IPaymentOutput[]} output_array
*/
  constructor(output_array: IPaymentOutput[]);
}
/**
* @category Wallet SDK
*/
export class PendingTransaction {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* List of unique addresses used by transaction inputs.
* This method can be used to determine addresses used by transaction inputs
* in order to select private keys needed for transaction signing.
* @returns {Array<any>}
*/
  addresses(): Array<any>;
/**
* Provides a list of UTXO entries used by the transaction.
* @returns {Array<any>}
*/
  getUtxoEntries(): Array<any>;
/**
* Creates and returns a signature for the input at the specified index.
* @param {number} input_index
* @param {PrivateKey} private_key
* @param {SighashType | undefined} [sighash_type]
* @returns {HexString}
*/
  createInputSignature(input_index: number, private_key: PrivateKey, sighash_type?: SighashType): HexString;
/**
* Sets a signature to the input at the specified index.
* @param {number} input_index
* @param {HexString | Uint8Array} signature_script
*/
  fillInput(input_index: number, signature_script: HexString | Uint8Array): void;
/**
* Signs the input at the specified index with the supplied private key
* and an optional SighashType.
* @param {number} input_index
* @param {PrivateKey} private_key
* @param {SighashType | undefined} [sighash_type]
*/
  signInput(input_index: number, private_key: PrivateKey, sighash_type?: SighashType): void;
/**
* Signs transaction with supplied [`Array`] or [`PrivateKey`] or an array of
* raw private key bytes (encoded as `Uint8Array` or as hex strings)
* @param {(PrivateKey | HexString | Uint8Array)[]} js_value
* @param {boolean | undefined} [check_fully_signed]
*/
  sign(js_value: (PrivateKey | HexString | Uint8Array)[], check_fully_signed?: boolean): void;
/**
* Submit transaction to the supplied [`RpcClient`]
* **IMPORTANT:** This method will remove UTXOs from the associated
* {@link UtxoContext} if one was used to create the transaction
* and will return UTXOs back to {@link UtxoContext} in case of
* a failed submission.
*
* # Important
*
* Make sure to consume the returned `txid` value. Always invoke this method
* as follows `let txid = await pendingTransaction.submit(rpc);`. If you do not
* consume the returned value and the rpc object is temporary, the GC will
* collect the `rpc` object passed to submit() potentially causing a panic.
*
* @see {@link RpcClient.submitTransaction}
* @param {RpcClient} wasm_rpc_client
* @returns {Promise<string>}
*/
  submit(wasm_rpc_client: RpcClient): Promise<string>;
/**
* Serializes the transaction to a pure JavaScript Object.
* The schema of the JavaScript object is defined by {@link ISerializableTransaction}.
* @see {@link ISerializableTransaction}
* @see {@link Transaction}, {@link ISerializableTransaction}
* @returns {ITransaction | Transaction}
*/
  serializeToObject(): ITransaction | Transaction;
/**
* Serializes the transaction to a JSON string.
* The schema of the JSON is defined by {@link ISerializableTransaction}.
* Once serialized, the transaction can be deserialized using {@link Transaction.deserializeFromJSON}.
* @see {@link Transaction}, {@link ISerializableTransaction}
* @returns {string}
*/
  serializeToJSON(): string;
/**
* Serializes the transaction to a "Safe" JSON schema where it converts all `bigint` values to `string` to avoid potential client-side precision loss.
* Once serialized, the transaction can be deserialized using {@link Transaction.deserializeFromSafeJSON}.
* @see {@link Transaction}, {@link ISerializableTransaction}
* @returns {string}
*/
  serializeToSafeJSON(): string;
/**
* Total aggregate input amount.
*/
  readonly aggregateInputAmount: bigint;
/**
* Total aggregate output amount.
*/
  readonly aggregateOutputAmount: bigint;
/**
* Change amount (if any).
*/
  readonly changeAmount: bigint;
/**
* Total transaction fees (network fees + priority fees).
*/
  readonly feeAmount: bigint;
/**
* Transaction Id
*/
  readonly id: string;
/**
* Calculated transaction mass.
*/
  readonly mass: bigint;
/**
* Minimum number of signatures required by the transaction.
* (as specified during the transaction creation).
*/
  readonly minimumSignatures: number;
/**
* Total amount transferred to the destination (aggregate output - change).
*/
  readonly paymentAmount: any;
/**
* Returns encapsulated network [`Transaction`]
*/
  readonly transaction: Transaction;
/**
* Transaction type ("batch" or "final").
*/
  readonly type: string;
}
/**
*/
export class PipeOptions {
  free(): void;
/**
* @param {boolean | undefined} [end]
*/
  constructor(end?: boolean);
/**
*/
  end?: boolean;
}
/**
* Represents a Kaspa header PoW manager
* @category Mining
*/
export class PoW {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {Header | IHeader | IRawHeader} header
* @param {bigint | undefined} [timestamp]
*/
  constructor(header: Header | IHeader | IRawHeader, timestamp?: bigint);
/**
* Checks if the computed target meets or exceeds the difficulty specified in the template.
* @returns A boolean indicating if it reached the target and a bigint representing the reached target.
* @param {bigint} nonce
* @returns {[boolean, bigint]}
*/
  checkWork(nonce: bigint): [boolean, bigint];
/**
* Can be used for parsing Stratum templates.
* @param {string} pre_pow_hash
* @param {bigint} timestamp
* @param {number | undefined} [target_bits]
* @returns {PoW}
*/
  static fromRaw(pre_pow_hash: string, timestamp: bigint, target_bits?: number): PoW;
/**
* Hash of the header without timestamp and nonce.
*/
  readonly prePoWHash: string;
/**
* The target based on the provided bits.
*/
  readonly target: bigint;
}
/**
* Data structure that envelops a Private Key.
* @category Wallet SDK
*/
export class PrivateKey {
  free(): void;
/**
* Create a new [`PrivateKey`] from a hex-encoded string.
* @param {string} key
*/
  constructor(key: string);
/**
* Returns the [`PrivateKey`] key encoded as a hex string.
* @returns {string}
*/
  toString(): string;
/**
* Generate a [`Keypair`] from this [`PrivateKey`].
* @returns {Keypair}
*/
  toKeypair(): Keypair;
/**
* @returns {PublicKey}
*/
  toPublicKey(): PublicKey;
/**
* Get the [`Address`] of the PublicKey generated from this PrivateKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = privateKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddress(network: NetworkType | NetworkId | string): Address;
/**
* Get `ECDSA` [`Address`] of the PublicKey generated from this PrivateKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = privateKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddressECDSA(network: NetworkType | NetworkId | string): Address;
}
/**
*
* Helper class to generate private keys from an extended private key (XPrv).
* This class accepts the master Kaspa XPrv string (e.g. `xprv1...`) and generates
* private keys for the receive and change paths given the pre-set parameters
* such as account index, multisig purpose and cosigner index.
*
* Please note that in Kaspa master private keys use `kprv` prefix.
*
* @see {@link PublicKeyGenerator}, {@link XPub}, {@link XPrv}, {@link Mnemonic}
* @category Wallet SDK
*/
export class PrivateKeyGenerator {
  free(): void;
/**
* @param {XPrv | string} xprv
* @param {boolean} is_multisig
* @param {bigint} account_index
* @param {number | undefined} [cosigner_index]
*/
  constructor(xprv: XPrv | string, is_multisig: boolean, account_index: bigint, cosigner_index?: number);
/**
* @param {number} index
* @returns {PrivateKey}
*/
  receiveKey(index: number): PrivateKey;
/**
* @param {number} index
* @returns {PrivateKey}
*/
  changeKey(index: number): PrivateKey;
}
/**
*/
export class ProcessSendOptions {
  free(): void;
/**
* @param {boolean | undefined} [swallow_errors]
*/
  constructor(swallow_errors?: boolean);
/**
*/
  swallow_errors?: boolean;
}
/**
* @category Wallet SDK
*/
export class PrvKeyDataInfo {
  free(): void;
/**
* @param {string} _name
*/
  setName(_name: string): void;
/**
*/
  readonly id: string;
/**
*/
  readonly isEncrypted: any;
/**
*/
  readonly name: any;
}
/**
* Data structure that envelopes a PublicKey.
* Only supports Schnorr-based addresses.
* @category Wallet SDK
*/
export class PublicKey {
  free(): void;
/**
* Create a new [`PublicKey`] from a hex-encoded string.
* @param {string} key
*/
  constructor(key: string);
/**
* @returns {string}
*/
  toString(): string;
/**
* Get the [`Address`] of this PublicKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = publicKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddress(network: NetworkType | NetworkId | string): Address;
/**
* Get `ECDSA` [`Address`] of this PublicKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = publicKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddressECDSA(network: NetworkType | NetworkId | string): Address;
/**
* @returns {XOnlyPublicKey}
*/
  toXOnlyPublicKey(): XOnlyPublicKey;
/**
* Compute a 4-byte key fingerprint for this public key as a hex string.
* Default implementation uses `RIPEMD160(SHA256(public_key))`.
* @returns {HexString | undefined}
*/
  fingerprint(): HexString | undefined;
}
/**
*
* Helper class to generate public keys from an extended public key (XPub)
* that has been derived up to the co-signer index.
*
* Please note that in Kaspa master public keys use `kpub` prefix.
*
* @see {@link PrivateKeyGenerator}, {@link XPub}, {@link XPrv}, {@link Mnemonic}
* @category Wallet SDK
*/
export class PublicKeyGenerator {
  free(): void;
/**
* @param {XPub | string} kpub
* @param {number | undefined} [cosigner_index]
* @returns {PublicKeyGenerator}
*/
  static fromXPub(kpub: XPub | string, cosigner_index?: number): PublicKeyGenerator;
/**
* @param {XPrv | string} xprv
* @param {boolean} is_multisig
* @param {bigint} account_index
* @param {number | undefined} [cosigner_index]
* @returns {PublicKeyGenerator}
*/
  static fromMasterXPrv(xprv: XPrv | string, is_multisig: boolean, account_index: bigint, cosigner_index?: number): PublicKeyGenerator;
/**
* Generate Receive Public Key derivations for a given range.
* @param {number} start
* @param {number} end
* @returns {(PublicKey | string)[]}
*/
  receivePubkeys(start: number, end: number): (PublicKey | string)[];
/**
* Generate a single Receive Public Key derivation at a given index.
* @param {number} index
* @returns {PublicKey}
*/
  receivePubkey(index: number): PublicKey;
/**
* Generate a range of Receive Public Key derivations and return them as strings.
* @param {number} start
* @param {number} end
* @returns {Array<string>}
*/
  receivePubkeysAsStrings(start: number, end: number): Array<string>;
/**
* Generate a single Receive Public Key derivation at a given index and return it as a string.
* @param {number} index
* @returns {string}
*/
  receivePubkeyAsString(index: number): string;
/**
* Generate Receive Address derivations for a given range.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} start
* @param {number} end
* @returns {Address[]}
*/
  receiveAddresses(networkType: NetworkType | NetworkId | string, start: number, end: number): Address[];
/**
* Generate a single Receive Address derivation at a given index.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} index
* @returns {Address}
*/
  receiveAddress(networkType: NetworkType | NetworkId | string, index: number): Address;
/**
* Generate a range of Receive Address derivations and return them as strings.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} start
* @param {number} end
* @returns {Array<string>}
*/
  receiveAddressAsStrings(networkType: NetworkType | NetworkId | string, start: number, end: number): Array<string>;
/**
* Generate a single Receive Address derivation at a given index and return it as a string.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} index
* @returns {string}
*/
  receiveAddressAsString(networkType: NetworkType | NetworkId | string, index: number): string;
/**
* Generate Change Public Key derivations for a given range.
* @param {number} start
* @param {number} end
* @returns {(PublicKey | string)[]}
*/
  changePubkeys(start: number, end: number): (PublicKey | string)[];
/**
* Generate a single Change Public Key derivation at a given index.
* @param {number} index
* @returns {PublicKey}
*/
  changePubkey(index: number): PublicKey;
/**
* Generate a range of Change Public Key derivations and return them as strings.
* @param {number} start
* @param {number} end
* @returns {Array<string>}
*/
  changePubkeysAsStrings(start: number, end: number): Array<string>;
/**
* Generate a single Change Public Key derivation at a given index and return it as a string.
* @param {number} index
* @returns {string}
*/
  changePubkeyAsString(index: number): string;
/**
* Generate Change Address derivations for a given range.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} start
* @param {number} end
* @returns {Address[]}
*/
  changeAddresses(networkType: NetworkType | NetworkId | string, start: number, end: number): Address[];
/**
* Generate a single Change Address derivation at a given index.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} index
* @returns {Address}
*/
  changeAddress(networkType: NetworkType | NetworkId | string, index: number): Address;
/**
* Generate a range of Change Address derivations and return them as strings.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} start
* @param {number} end
* @returns {Array<string>}
*/
  changeAddressAsStrings(networkType: NetworkType | NetworkId | string, start: number, end: number): Array<string>;
/**
* Generate a single Change Address derivation at a given index and return it as a string.
* @param {NetworkType | NetworkId | string} networkType
* @param {number} index
* @returns {string}
*/
  changeAddressAsString(networkType: NetworkType | NetworkId | string, index: number): string;
/**
* @returns {string}
*/
  toString(): string;
}
/**
*
* Resolver is a client for obtaining public Kaspa wRPC URL.
*
* Resolver queries a list of public Kaspa Resolver URLs using HTTP to fetch
* wRPC endpoints for the given encoding, network identifier and other
* parameters. It then provides this information to the {@link RpcClient}.
*
* Each time {@link RpcClient} disconnects, it will query the resolver
* to fetch a new wRPC URL.
*
* ```javascript
* // using integrated public URLs
* let rpc = RpcClient({
*     resolver: new Resolver(),
*     networkId : "mainnet"
* });
*
* // specifying custom resolver URLs
* let rpc = RpcClient({
*     resolver: new Resolver({urls: ["<resolver-url>",...]}),
*     networkId : "mainnet"
* });
* ```
*
* @see {@link IResolverConfig}, {@link IResolverConnect}, {@link RpcClient}
* @category Node RPC
*/
export class Resolver {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Fetches a public Kaspa wRPC endpoint for the given encoding and network identifier.
* @see {@link Encoding}, {@link NetworkId}, {@link Node}
* @param {Encoding} encoding
* @param {NetworkId | string} network_id
* @returns {Promise<NodeDescriptor>}
*/
  getNode(encoding: Encoding, network_id: NetworkId | string): Promise<NodeDescriptor>;
/**
* Fetches a public Kaspa wRPC endpoint URL for the given encoding and network identifier.
* @see {@link Encoding}, {@link NetworkId}
* @param {Encoding} encoding
* @param {NetworkId | string} network_id
* @returns {Promise<string>}
*/
  getUrl(encoding: Encoding, network_id: NetworkId | string): Promise<string>;
/**
* Connect to a public Kaspa wRPC endpoint for the given encoding and network identifier
* supplied via {@link IResolverConnect} interface.
* @see {@link IResolverConnect}, {@link RpcClient}
* @param {IResolverConnect | NetworkId | string} options
* @returns {Promise<RpcClient>}
*/
  connect(options: IResolverConnect | NetworkId | string): Promise<RpcClient>;
/**
* Creates a new Resolver client with the given
* configuration supplied as {@link IResolverConfig}
* interface. If not supplied, the default configuration
* containing a list of community-operated resolvers
* will be used.
* @param {IResolverConfig | string[] | undefined} [args]
*/
  constructor(args?: IResolverConfig | string[]);
/**
* List of public Kaspa Resolver URLs.
*/
  readonly urls: string[] | undefined;
}
/**
*
*
* Kaspa RPC client uses ([wRPC](https://github.com/workflow-rs/workflow-rs/tree/master/rpc))
* interface to connect directly with Kaspa Node. wRPC supports
* two types of encodings: `borsh` (binary, default) and `json`.
*
* There are two ways to connect: Directly to any Kaspa Node or to a
* community-maintained public node infrastructure using the {@link Resolver} class.
*
* **Connecting to a public node using a resolver**
*
* ```javascript
* let rpc = new RpcClient({
*    resolver : new Resolver(),
*    networkId : "mainnet",
* });
*
* await rpc.connect();
* ```
*
* **Connecting to a Kaspa Node directly**
*
* ```javascript
* let rpc = new RpcClient({
*    // if port is not provided it will default
*    // to the default port for the networkId
*    url : "127.0.0.1",
*    networkId : "mainnet",
* });
* ```
*
* **Example usage**
*
* ```javascript
*
* // Create a new RPC client with a URL
* let rpc = new RpcClient({ url : "wss://<node-wrpc-address>" });
*
* // Create a new RPC client with a resolver
* // (networkId is required when using a resolver)
* let rpc = new RpcClient({
*     resolver : new Resolver(),
*     networkId : "mainnet",
* });
*
* rpc.addEventListener("connect", async (event) => {
*     console.log("Connected to", rpc.url);
*     await rpc.subscribeDaaScore();
* });
*
* rpc.addEventListener("disconnect", (event) => {
*     console.log("Disconnected from", rpc.url);
* });
*
* try {
*     await rpc.connect();
* } catch(err) {
*     console.log("Error connecting:", err);
* }
*
* ```
*
* You can register event listeners to receive notifications from the RPC client
* using {@link RpcClient.addEventListener} and {@link RpcClient.removeEventListener} functions.
*
* **IMPORTANT:** If RPC is disconnected, upon reconnection you do not need
* to re-register event listeners, but your have to re-subscribe for Kaspa node
* notifications:
*
* ```typescript
* rpc.addEventListener("connect", async (event) => {
*     console.log("Connected to", rpc.url);
*     // re-subscribe each time we connect
*     await rpc.subscribeDaaScore();
*     // ... perform wallet address subscriptions
* });
*
* ```
*
* If using NodeJS, it is important that {@link RpcClient.disconnect} is called before
* the process exits to ensure that the WebSocket connection is properly closed.
* Failure to do this will prevent the process from exiting.
*
* @category Node RPC
*/
export class RpcClient {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Retrieves the current number of blocks in the Kaspa BlockDAG.
* This is not a block count, not a "block height" and can not be
* used for transaction validation.
* Returned information: Current block count.
*@see {@link IGetBlockCountRequest}, {@link IGetBlockCountResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetBlockCountRequest | undefined} [request]
* @returns {Promise<IGetBlockCountResponse>}
*/
  getBlockCount(request?: IGetBlockCountRequest): Promise<IGetBlockCountResponse>;
/**
* Provides information about the Directed Acyclic Graph (DAG)
* structure of the Kaspa BlockDAG.
* Returned information: Number of blocks in the DAG,
* number of tips in the DAG, hash of the selected parent block,
* difficulty of the selected parent block, selected parent block
* blue score, selected parent block time.
*@see {@link IGetBlockDagInfoRequest}, {@link IGetBlockDagInfoResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetBlockDagInfoRequest | undefined} [request]
* @returns {Promise<IGetBlockDagInfoResponse>}
*/
  getBlockDagInfo(request?: IGetBlockDagInfoRequest): Promise<IGetBlockDagInfoResponse>;
/**
* Returns the total current coin supply of Kaspa network.
* Returned information: Total coin supply.
*@see {@link IGetCoinSupplyRequest}, {@link IGetCoinSupplyResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetCoinSupplyRequest | undefined} [request]
* @returns {Promise<IGetCoinSupplyResponse>}
*/
  getCoinSupply(request?: IGetCoinSupplyRequest): Promise<IGetCoinSupplyResponse>;
/**
* Retrieves information about the peers connected to the Kaspa node.
* Returned information: Peer ID, IP address and port, connection
* status, protocol version.
*@see {@link IGetConnectedPeerInfoRequest}, {@link IGetConnectedPeerInfoResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetConnectedPeerInfoRequest | undefined} [request]
* @returns {Promise<IGetConnectedPeerInfoResponse>}
*/
  getConnectedPeerInfo(request?: IGetConnectedPeerInfoRequest): Promise<IGetConnectedPeerInfoResponse>;
/**
* Retrieves general information about the Kaspa node.
* Returned information: Version of the Kaspa node, protocol
* version, network identifier.
* This call is primarily used by gRPC clients.
* For wRPC clients, use {@link RpcClient.getServerInfo}.
*@see {@link IGetInfoRequest}, {@link IGetInfoResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetInfoRequest | undefined} [request]
* @returns {Promise<IGetInfoResponse>}
*/
  getInfo(request?: IGetInfoRequest): Promise<IGetInfoResponse>;
/**
* Provides a list of addresses of known peers in the Kaspa
* network that the node can potentially connect to.
* Returned information: List of peer addresses.
*@see {@link IGetPeerAddressesRequest}, {@link IGetPeerAddressesResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetPeerAddressesRequest | undefined} [request]
* @returns {Promise<IGetPeerAddressesResponse>}
*/
  getPeerAddresses(request?: IGetPeerAddressesRequest): Promise<IGetPeerAddressesResponse>;
/**
* Retrieves various metrics and statistics related to the
* performance and status of the Kaspa node.
* Returned information: Memory usage, CPU usage, network activity.
*@see {@link IGetMetricsRequest}, {@link IGetMetricsResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetMetricsRequest | undefined} [request]
* @returns {Promise<IGetMetricsResponse>}
*/
  getMetrics(request?: IGetMetricsRequest): Promise<IGetMetricsResponse>;
/**
* Retrieves current number of network connections
*@see {@link IGetConnectionsRequest}, {@link IGetConnectionsResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetConnectionsRequest | undefined} [request]
* @returns {Promise<IGetConnectionsResponse>}
*/
  getConnections(request?: IGetConnectionsRequest): Promise<IGetConnectionsResponse>;
/**
* Retrieves the current sink block, which is the block with
* the highest cumulative difficulty in the Kaspa BlockDAG.
* Returned information: Sink block hash, sink block height.
*@see {@link IGetSinkRequest}, {@link IGetSinkResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetSinkRequest | undefined} [request]
* @returns {Promise<IGetSinkResponse>}
*/
  getSink(request?: IGetSinkRequest): Promise<IGetSinkResponse>;
/**
* Returns the blue score of the current sink block, indicating
* the total amount of work that has been done on the main chain
* leading up to that block.
* Returned information: Blue score of the sink block.
*@see {@link IGetSinkBlueScoreRequest}, {@link IGetSinkBlueScoreResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetSinkBlueScoreRequest | undefined} [request]
* @returns {Promise<IGetSinkBlueScoreResponse>}
*/
  getSinkBlueScore(request?: IGetSinkBlueScoreRequest): Promise<IGetSinkBlueScoreResponse>;
/**
* Tests the connection and responsiveness of a Kaspa node.
* Returned information: None.
*@see {@link IPingRequest}, {@link IPingResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IPingRequest | undefined} [request]
* @returns {Promise<IPingResponse>}
*/
  ping(request?: IPingRequest): Promise<IPingResponse>;
/**
* Gracefully shuts down the Kaspa node.
* Returned information: None.
*@see {@link IShutdownRequest}, {@link IShutdownResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IShutdownRequest | undefined} [request]
* @returns {Promise<IShutdownResponse>}
*/
  shutdown(request?: IShutdownRequest): Promise<IShutdownResponse>;
/**
* Retrieves information about the Kaspa server.
* Returned information: Version of the Kaspa server, protocol
* version, network identifier.
*@see {@link IGetServerInfoRequest}, {@link IGetServerInfoResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetServerInfoRequest | undefined} [request]
* @returns {Promise<IGetServerInfoResponse>}
*/
  getServerInfo(request?: IGetServerInfoRequest): Promise<IGetServerInfoResponse>;
/**
* Obtains basic information about the synchronization status of the Kaspa node.
* Returned information: Syncing status.
*@see {@link IGetSyncStatusRequest}, {@link IGetSyncStatusResponse}
*@throws `string` on an RPC error or a server-side error.
* @param {IGetSyncStatusRequest | undefined} [request]
* @returns {Promise<IGetSyncStatusResponse>}
*/
  getSyncStatus(request?: IGetSyncStatusRequest): Promise<IGetSyncStatusResponse>;
/**
* Adds a peer to the Kaspa node's list of known peers.
* Returned information: None.
*@see {@link IAddPeerRequest}, {@link IAddPeerResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IAddPeerRequest} request
* @returns {Promise<IAddPeerResponse>}
*/
  addPeer(request: IAddPeerRequest): Promise<IAddPeerResponse>;
/**
* Bans a peer from connecting to the Kaspa node for a specified duration.
* Returned information: None.
*@see {@link IBanRequest}, {@link IBanResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IBanRequest} request
* @returns {Promise<IBanResponse>}
*/
  ban(request: IBanRequest): Promise<IBanResponse>;
/**
* Estimates the network's current hash rate in hashes per second.
* Returned information: Estimated network hashes per second.
*@see {@link IEstimateNetworkHashesPerSecondRequest}, {@link IEstimateNetworkHashesPerSecondResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IEstimateNetworkHashesPerSecondRequest} request
* @returns {Promise<IEstimateNetworkHashesPerSecondResponse>}
*/
  estimateNetworkHashesPerSecond(request: IEstimateNetworkHashesPerSecondRequest): Promise<IEstimateNetworkHashesPerSecondResponse>;
/**
* Retrieves the balance of a specific address in the Kaspa BlockDAG.
* Returned information: Balance of the address.
*@see {@link IGetBalanceByAddressRequest}, {@link IGetBalanceByAddressResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetBalanceByAddressRequest} request
* @returns {Promise<IGetBalanceByAddressResponse>}
*/
  getBalanceByAddress(request: IGetBalanceByAddressRequest): Promise<IGetBalanceByAddressResponse>;
/**
* Retrieves balances for multiple addresses in the Kaspa BlockDAG.
* Returned information: Balances of the addresses.
*@see {@link IGetBalancesByAddressesRequest}, {@link IGetBalancesByAddressesResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetBalancesByAddressesRequest | Address[] | string[]} request
* @returns {Promise<IGetBalancesByAddressesResponse>}
*/
  getBalancesByAddresses(request: IGetBalancesByAddressesRequest | Address[] | string[]): Promise<IGetBalancesByAddressesResponse>;
/**
* Retrieves a specific block from the Kaspa BlockDAG.
* Returned information: Block information.
*@see {@link IGetBlockRequest}, {@link IGetBlockResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetBlockRequest} request
* @returns {Promise<IGetBlockResponse>}
*/
  getBlock(request: IGetBlockRequest): Promise<IGetBlockResponse>;
/**
* Retrieves multiple blocks from the Kaspa BlockDAG.
* Returned information: List of block information.
*@see {@link IGetBlocksRequest}, {@link IGetBlocksResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetBlocksRequest} request
* @returns {Promise<IGetBlocksResponse>}
*/
  getBlocks(request: IGetBlocksRequest): Promise<IGetBlocksResponse>;
/**
* Generates a new block template for mining.
* Returned information: Block template information.
*@see {@link IGetBlockTemplateRequest}, {@link IGetBlockTemplateResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetBlockTemplateRequest} request
* @returns {Promise<IGetBlockTemplateResponse>}
*/
  getBlockTemplate(request: IGetBlockTemplateRequest): Promise<IGetBlockTemplateResponse>;
/**
* Checks if block is blue or not.
* Returned information: Block blueness.
*@see {@link IGetCurrentBlockColorRequest}, {@link IGetCurrentBlockColorResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetCurrentBlockColorRequest} request
* @returns {Promise<IGetCurrentBlockColorResponse>}
*/
  getCurrentBlockColor(request: IGetCurrentBlockColorRequest): Promise<IGetCurrentBlockColorResponse>;
/**
* Retrieves the estimated DAA (Difficulty Adjustment Algorithm)
* score timestamp estimate.
* Returned information: DAA score timestamp estimate.
*@see {@link IGetDaaScoreTimestampEstimateRequest}, {@link IGetDaaScoreTimestampEstimateResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetDaaScoreTimestampEstimateRequest} request
* @returns {Promise<IGetDaaScoreTimestampEstimateResponse>}
*/
  getDaaScoreTimestampEstimate(request: IGetDaaScoreTimestampEstimateRequest): Promise<IGetDaaScoreTimestampEstimateResponse>;
/**
* Feerate estimates
*@see {@link IGetFeeEstimateRequest}, {@link IGetFeeEstimateResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetFeeEstimateRequest} request
* @returns {Promise<IGetFeeEstimateResponse>}
*/
  getFeeEstimate(request: IGetFeeEstimateRequest): Promise<IGetFeeEstimateResponse>;
/**
* Feerate estimates (experimental)
*@see {@link IGetFeeEstimateExperimentalRequest}, {@link IGetFeeEstimateExperimentalResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetFeeEstimateExperimentalRequest} request
* @returns {Promise<IGetFeeEstimateExperimentalResponse>}
*/
  getFeeEstimateExperimental(request: IGetFeeEstimateExperimentalRequest): Promise<IGetFeeEstimateExperimentalResponse>;
/**
* Retrieves the current network configuration.
* Returned information: Current network configuration.
*@see {@link IGetCurrentNetworkRequest}, {@link IGetCurrentNetworkResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetCurrentNetworkRequest} request
* @returns {Promise<IGetCurrentNetworkResponse>}
*/
  getCurrentNetwork(request: IGetCurrentNetworkRequest): Promise<IGetCurrentNetworkResponse>;
/**
* Retrieves block headers from the Kaspa BlockDAG.
* Returned information: List of block headers.
*@see {@link IGetHeadersRequest}, {@link IGetHeadersResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetHeadersRequest} request
* @returns {Promise<IGetHeadersResponse>}
*/
  getHeaders(request: IGetHeadersRequest): Promise<IGetHeadersResponse>;
/**
* Retrieves mempool entries from the Kaspa node's mempool.
* Returned information: List of mempool entries.
*@see {@link IGetMempoolEntriesRequest}, {@link IGetMempoolEntriesResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetMempoolEntriesRequest} request
* @returns {Promise<IGetMempoolEntriesResponse>}
*/
  getMempoolEntries(request: IGetMempoolEntriesRequest): Promise<IGetMempoolEntriesResponse>;
/**
* Retrieves mempool entries associated with specific addresses.
* Returned information: List of mempool entries.
*@see {@link IGetMempoolEntriesByAddressesRequest}, {@link IGetMempoolEntriesByAddressesResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetMempoolEntriesByAddressesRequest} request
* @returns {Promise<IGetMempoolEntriesByAddressesResponse>}
*/
  getMempoolEntriesByAddresses(request: IGetMempoolEntriesByAddressesRequest): Promise<IGetMempoolEntriesByAddressesResponse>;
/**
* Retrieves a specific mempool entry by transaction ID.
* Returned information: Mempool entry information.
*@see {@link IGetMempoolEntryRequest}, {@link IGetMempoolEntryResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetMempoolEntryRequest} request
* @returns {Promise<IGetMempoolEntryResponse>}
*/
  getMempoolEntry(request: IGetMempoolEntryRequest): Promise<IGetMempoolEntryResponse>;
/**
* Retrieves information about a subnetwork in the Kaspa BlockDAG.
* Returned information: Subnetwork information.
*@see {@link IGetSubnetworkRequest}, {@link IGetSubnetworkResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetSubnetworkRequest} request
* @returns {Promise<IGetSubnetworkResponse>}
*/
  getSubnetwork(request: IGetSubnetworkRequest): Promise<IGetSubnetworkResponse>;
/**
* Retrieves unspent transaction outputs (UTXOs) associated with
* specific addresses.
* Returned information: List of UTXOs.
*@see {@link IGetUtxosByAddressesRequest}, {@link IGetUtxosByAddressesResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetUtxosByAddressesRequest | Address[] | string[]} request
* @returns {Promise<IGetUtxosByAddressesResponse>}
*/
  getUtxosByAddresses(request: IGetUtxosByAddressesRequest | Address[] | string[]): Promise<IGetUtxosByAddressesResponse>;
/**
* Retrieves the virtual chain corresponding to a specified block hash.
* Returned information: Virtual chain information.
*@see {@link IGetVirtualChainFromBlockRequest}, {@link IGetVirtualChainFromBlockResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IGetVirtualChainFromBlockRequest} request
* @returns {Promise<IGetVirtualChainFromBlockResponse>}
*/
  getVirtualChainFromBlock(request: IGetVirtualChainFromBlockRequest): Promise<IGetVirtualChainFromBlockResponse>;
/**
* Resolves a finality conflict in the Kaspa BlockDAG.
* Returned information: None.
*@see {@link IResolveFinalityConflictRequest}, {@link IResolveFinalityConflictResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IResolveFinalityConflictRequest} request
* @returns {Promise<IResolveFinalityConflictResponse>}
*/
  resolveFinalityConflict(request: IResolveFinalityConflictRequest): Promise<IResolveFinalityConflictResponse>;
/**
* Submits a block to the Kaspa network.
* Returned information: None.
*@see {@link ISubmitBlockRequest}, {@link ISubmitBlockResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {ISubmitBlockRequest} request
* @returns {Promise<ISubmitBlockResponse>}
*/
  submitBlock(request: ISubmitBlockRequest): Promise<ISubmitBlockResponse>;
/**
* Submits a transaction to the Kaspa network.
* Returned information: Submitted Transaction Id.
*@see {@link ISubmitTransactionRequest}, {@link ISubmitTransactionResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {ISubmitTransactionRequest} request
* @returns {Promise<ISubmitTransactionResponse>}
*/
  submitTransaction(request: ISubmitTransactionRequest): Promise<ISubmitTransactionResponse>;
/**
* Submits an RBF transaction to the Kaspa network.
* Returned information: Submitted Transaction Id, Transaction that was replaced.
*@see {@link ISubmitTransactionReplacementRequest}, {@link ISubmitTransactionReplacementResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {ISubmitTransactionReplacementRequest} request
* @returns {Promise<ISubmitTransactionReplacementResponse>}
*/
  submitTransactionReplacement(request: ISubmitTransactionReplacementRequest): Promise<ISubmitTransactionReplacementResponse>;
/**
* Unbans a previously banned peer, allowing it to connect
* to the Kaspa node again.
* Returned information: None.
*@see {@link IUnbanRequest}, {@link IUnbanResponse}
*@throws `string` on an RPC error, a server-side error or when supplying incorrect arguments.
* @param {IUnbanRequest} request
* @returns {Promise<IUnbanResponse>}
*/
  unban(request: IUnbanRequest): Promise<IUnbanResponse>;
/**
* Manage subscription for a block added notification event.
* Block added notification event is produced when a new
* block is added to the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribeBlockAdded(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribeBlockAdded(): Promise<void>;
/**
* Manage subscription for a finality conflict notification event.
* Finality conflict notification event is produced when a finality
* conflict occurs in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribeFinalityConflict(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribeFinalityConflict(): Promise<void>;
/**
* Manage subscription for a finality conflict resolved notification event.
* Finality conflict resolved notification event is produced when a finality
* conflict in the Kaspa BlockDAG is resolved.
* @returns {Promise<void>}
*/
  subscribeFinalityConflictResolved(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribeFinalityConflictResolved(): Promise<void>;
/**
* Manage subscription for a sink blue score changed notification event.
* Sink blue score changed notification event is produced when the blue
* score of the sink block changes in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribeSinkBlueScoreChanged(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribeSinkBlueScoreChanged(): Promise<void>;
/**
* Manage subscription for a pruning point UTXO set override notification event.
* Pruning point UTXO set override notification event is produced when the
* UTXO set override for the pruning point changes in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribePruningPointUtxoSetOverride(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribePruningPointUtxoSetOverride(): Promise<void>;
/**
* Manage subscription for a new block template notification event.
* New block template notification event is produced when a new block
* template is generated for mining in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribeNewBlockTemplate(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  unsubscribeNewBlockTemplate(): Promise<void>;
/**
* Manage subscription for a virtual DAA score changed notification event.
* Virtual DAA score changed notification event is produced when the virtual
* Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  subscribeVirtualDaaScoreChanged(): Promise<void>;
/**
* Manage subscription for a virtual DAA score changed notification event.
* Virtual DAA score changed notification event is produced when the virtual
* Difficulty Adjustment Algorithm (DAA) score changes in the Kaspa BlockDAG.
* @returns {Promise<void>}
*/
  unsubscribeVirtualDaaScoreChanged(): Promise<void>;
/**
* Subscribe for a UTXOs changed notification event.
* UTXOs changed notification event is produced when the set
* of unspent transaction outputs (UTXOs) changes in the
* Kaspa BlockDAG. The event notification will be scoped to the
* provided list of addresses.
* @param {(Address | string)[]} addresses
* @returns {Promise<void>}
*/
  subscribeUtxosChanged(addresses: (Address | string)[]): Promise<void>;
/**
* Unsubscribe from UTXOs changed notification event
* for a specific set of addresses.
* @param {(Address | string)[]} addresses
* @returns {Promise<void>}
*/
  unsubscribeUtxosChanged(addresses: (Address | string)[]): Promise<void>;
/**
* Manage subscription for a virtual chain changed notification event.
* Virtual chain changed notification event is produced when the virtual
* chain changes in the Kaspa BlockDAG.
* @param {boolean} include_accepted_transaction_ids
* @returns {Promise<void>}
*/
  subscribeVirtualChainChanged(include_accepted_transaction_ids: boolean): Promise<void>;
/**
* Manage subscription for a virtual chain changed notification event.
* Virtual chain changed notification event is produced when the virtual
* chain changes in the Kaspa BlockDAG.
* @param {boolean} include_accepted_transaction_ids
* @returns {Promise<void>}
*/
  unsubscribeVirtualChainChanged(include_accepted_transaction_ids: boolean): Promise<void>;
/**
* @param {Encoding} encoding
* @param {NetworkType | NetworkId | string} network
* @returns {number}
*/
  static defaultPort(encoding: Encoding, network: NetworkType | NetworkId | string): number;
/**
* Constructs an WebSocket RPC URL given the partial URL or an IP, RPC encoding
* and a network type.
*
* # Arguments
*
* * `url` - Partial URL or an IP address
* * `encoding` - RPC encoding
* * `network_type` - Network type
* @param {string} url
* @param {Encoding} encoding
* @param {NetworkId} network
* @returns {string}
*/
  static parseUrl(url: string, encoding: Encoding, network: NetworkId): string;
/**
*
* Create a new RPC client with optional {@link Encoding} and a `url`.
*
* @see {@link IRpcConfig} interface for more details.
* @param {IRpcConfig | undefined} [config]
*/
  constructor(config?: IRpcConfig);
/**
* Set the resolver for the RPC client.
* This setting will take effect on the next connection.
* @param {Resolver} resolver
*/
  setResolver(resolver: Resolver): void;
/**
* Set the network id for the RPC client.
* This setting will take effect on the next connection.
* @param {NetworkId} network_id
*/
  setNetworkId(network_id: NetworkId): void;
/**
* Connect to the Kaspa RPC server. This function starts a background
* task that connects and reconnects to the server if the connection
* is terminated.  Use [`disconnect()`](Self::disconnect()) to
* terminate the connection.
* @see {@link IConnectOptions} interface for more details.
* @param {IConnectOptions | undefined | undefined} [args]
* @returns {Promise<void>}
*/
  connect(args?: IConnectOptions | undefined): Promise<void>;
/**
* Disconnect from the Kaspa RPC server.
* @returns {Promise<void>}
*/
  disconnect(): Promise<void>;
/**
* Start background RPC services (automatically started when invoking {@link RpcClient.connect}).
* @returns {Promise<void>}
*/
  start(): Promise<void>;
/**
* Stop background RPC services (automatically stopped when invoking {@link RpcClient.disconnect}).
* @returns {Promise<void>}
*/
  stop(): Promise<void>;
/**
* Triggers a disconnection on the underlying WebSocket
* if the WebSocket is in connected state.
* This is intended for debug purposes only.
* Can be used to test application reconnection logic.
*/
  triggerAbort(): void;
/**
*
* Unregister an event listener.
* This function will remove the callback for the specified event.
* If the `callback` is not supplied, all callbacks will be
* removed for the specified event.
*
* @see {@link RpcClient.addEventListener}
* @param {RpcEventType | string} event
* @param {RpcEventCallback | undefined} [callback]
*/
  removeEventListener(event: RpcEventType | string, callback?: RpcEventCallback): void;
/**
*
* Unregister a single event listener callback from all events.
*
*
* @param {RpcEventCallback} callback
*/
  clearEventListener(callback: RpcEventCallback): void;
/**
*
* Unregister all notification callbacks for all events.
*/
  removeAllEventListeners(): void;
/**
* The current protocol encoding.
*/
  readonly encoding: string;
/**
* The current connection status of the RPC client.
*/
  readonly isConnected: boolean;
/**
* Optional: Resolver node id.
*/
  readonly nodeId: string | undefined;
/**
* Current rpc resolver
*/
  readonly resolver: Resolver | undefined;
/**
* The current URL of the RPC client.
*/
  readonly url: string | undefined;
}
/**
* ScriptBuilder provides a facility for building custom scripts. It allows
* you to push opcodes, ints, and data while respecting canonical encoding. In
* general it does not ensure the script will execute correctly, however any
* data pushes which would exceed the maximum allowed script engine limits and
* are therefore guaranteed not to execute will not be pushed and will result in
* the Script function returning an error.
* @category Consensus
*/
export class ScriptBuilder {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  constructor();
/**
* Creates a new ScriptBuilder over an existing script.
* Supplied script can be represented as an `Uint8Array` or a `HexString`.
* @param {HexString | Uint8Array} script
* @returns {ScriptBuilder}
*/
  static fromScript(script: HexString | Uint8Array): ScriptBuilder;
/**
* Pushes the passed opcode to the end of the script. The script will not
* be modified if pushing the opcode would cause the script to exceed the
* maximum allowed script engine size.
* @param {number} op
* @returns {ScriptBuilder}
*/
  addOp(op: number): ScriptBuilder;
/**
* Adds the passed opcodes to the end of the script.
* Supplied opcodes can be represented as an `Uint8Array` or a `HexString`.
* @param {HexString | Uint8Array} opcodes
* @returns {ScriptBuilder}
*/
  addOps(opcodes: HexString | Uint8Array): ScriptBuilder;
/**
* AddData pushes the passed data to the end of the script. It automatically
* chooses canonical opcodes depending on the length of the data.
*
* A zero length buffer will lead to a push of empty data onto the stack (Op0 = OpFalse)
* and any push of data greater than [`MAX_SCRIPT_ELEMENT_SIZE`](kaspa_txscript::MAX_SCRIPT_ELEMENT_SIZE) will not modify
* the script since that is not allowed by the script engine.
*
* Also, the script will not be modified if pushing the data would cause the script to
* exceed the maximum allowed script engine size [`MAX_SCRIPTS_SIZE`](kaspa_txscript::MAX_SCRIPTS_SIZE).
* @param {HexString | Uint8Array} data
* @returns {ScriptBuilder}
*/
  addData(data: HexString | Uint8Array): ScriptBuilder;
/**
* @param {bigint} value
* @returns {ScriptBuilder}
*/
  addI64(value: bigint): ScriptBuilder;
/**
* @param {bigint} lock_time
* @returns {ScriptBuilder}
*/
  addLockTime(lock_time: bigint): ScriptBuilder;
/**
* @param {bigint} sequence
* @returns {ScriptBuilder}
*/
  addSequence(sequence: bigint): ScriptBuilder;
/**
* @param {HexString | Uint8Array} data
* @returns {number}
*/
  static canonicalDataSize(data: HexString | Uint8Array): number;
/**
* Get script bytes represented by a hex string.
* @returns {HexString}
*/
  toString(): HexString;
/**
* Drains (empties) the script builder, returning the
* script bytes represented by a hex string.
* @returns {HexString}
*/
  drain(): HexString;
/**
* Creates an equivalent pay-to-script-hash script.
* Can be used to create an P2SH address.
* @see {@link addressFromScriptPublicKey}
* @returns {ScriptPublicKey}
*/
  createPayToScriptHashScript(): ScriptPublicKey;
/**
* Generates a signature script that fits a pay-to-script-hash script.
* @param {HexString | Uint8Array} signature
* @returns {HexString}
*/
  encodePayToScriptHashSignatureScript(signature: HexString | Uint8Array): HexString;
/**
* @param {IHexViewConfig | undefined} [args]
* @returns {string}
*/
  hexView(args?: IHexViewConfig): string;
}
/**
* Represents a Kaspad ScriptPublicKey
* @category Consensus
*/
export class ScriptPublicKey {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {number} version
* @param {any} script
*/
  constructor(version: number, script: any);
/**
*/
  readonly script: string;
/**
*/
  version: number;
}
/**
*/
export class SetAadOptions {
  free(): void;
/**
* @param {Function} flush
* @param {number} plaintext_length
* @param {Function} transform
*/
  constructor(flush: Function, plaintext_length: number, transform: Function);
/**
*/
  flush: Function;
/**
*/
  readonly plaintextLength: number;
/**
*/
  plaintext_length: number;
/**
*/
  transform: Function;
}
/**
*/
export class SigHashType {
  free(): void;
}
/**
* Wallet file storage interface
* @category Wallet SDK
*/
export class Storage {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  readonly filename: string;
}
/**
*/
export class StreamTransformOptions {
  free(): void;
/**
* @param {Function} flush
* @param {Function} transform
*/
  constructor(flush: Function, transform: Function);
/**
*/
  flush: Function;
/**
*/
  transform: Function;
}
/**
* Represents a Kaspa transaction.
* This is an artificial construct that includes additional
* transaction-related data such as additional data from UTXOs
* used by transaction inputs.
* @category Consensus
*/
export class Transaction {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Determines whether or not a transaction is a coinbase transaction. A coinbase
* transaction is a special transaction created by miners that distributes fees and block subsidy
* to the previous blocks' miners, and specifies the script_pub_key that will be used to pay the current
* miner in future blocks.
* @returns {boolean}
*/
  is_coinbase(): boolean;
/**
* Recompute and finalize the tx id based on updated tx fields
* @returns {Hash}
*/
  finalize(): Hash;
/**
* @param {ITransaction | Transaction} js_value
*/
  constructor(js_value: ITransaction | Transaction);
/**
* Returns a list of unique addresses used by transaction inputs.
* This method can be used to determine addresses used by transaction inputs
* in order to select private keys needed for transaction signing.
* @param {NetworkType | NetworkId | string} network_type
* @returns {Address[]}
*/
  addresses(network_type: NetworkType | NetworkId | string): Address[];
/**
* Serializes the transaction to a pure JavaScript Object.
* The schema of the JavaScript object is defined by {@link ISerializableTransaction}.
* @see {@link ISerializableTransaction}
* @returns {ISerializableTransaction}
*/
  serializeToObject(): ISerializableTransaction;
/**
* Serializes the transaction to a JSON string.
* The schema of the JSON is defined by {@link ISerializableTransaction}.
* @returns {string}
*/
  serializeToJSON(): string;
/**
* Serializes the transaction to a "Safe" JSON schema where it converts all `bigint` values to `string` to avoid potential client-side precision loss.
* @returns {string}
*/
  serializeToSafeJSON(): string;
/**
* Deserialize the {@link Transaction} Object from a pure JavaScript Object.
* @param {any} js_value
* @returns {Transaction}
*/
  static deserializeFromObject(js_value: any): Transaction;
/**
* Deserialize the {@link Transaction} Object from a JSON string.
* @param {string} json
* @returns {Transaction}
*/
  static deserializeFromJSON(json: string): Transaction;
/**
* Deserialize the {@link Transaction} Object from a "Safe" JSON schema where all `bigint` values are represented as `string`.
* @param {string} json
* @returns {Transaction}
*/
  static deserializeFromSafeJSON(json: string): Transaction;
/**
*/
  gas: bigint;
/**
* Returns the transaction ID
*/
  readonly id: string;
/**
*/
  inputs: (ITransactionInput | TransactionInput)[];
/**
*/
  lockTime: bigint;
/**
*/
  mass: bigint;
/**
*/
  outputs: (ITransactionOutput | TransactionOutput)[];
/**
*/
  payload: any;
/**
*/
  subnetworkId: any;
/**
*/
  version: number;
}
/**
* Represents a Kaspa transaction input
* @category Consensus
*/
export class TransactionInput {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {ITransactionInput | TransactionInput} value
*/
  constructor(value: ITransactionInput | TransactionInput);
/**
*/
  previousOutpoint: any;
/**
*/
  sequence: bigint;
/**
*/
  sigOpCount: number;
/**
*/
  signatureScript: any;
/**
*/
  readonly utxo: UtxoEntryReference | undefined;
}
/**
* Represents a Kaspa transaction outpoint.
* NOTE: This struct is immutable - to create a custom outpoint
* use the `TransactionOutpoint::new` constructor. (in JavaScript
* use `new TransactionOutpoint(transactionId, index)`).
* @category Consensus
*/
export class TransactionOutpoint {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {Hash} transaction_id
* @param {number} index
*/
  constructor(transaction_id: Hash, index: number);
/**
* @returns {string}
*/
  getId(): string;
/**
*/
  readonly index: number;
/**
*/
  readonly transactionId: string;
}
/**
* Represents a Kaspad transaction output
* @category Consensus
*/
export class TransactionOutput {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* TransactionOutput constructor
* @param {bigint} value
* @param {ScriptPublicKey} script_public_key
*/
  constructor(value: bigint, script_public_key: ScriptPublicKey);
/**
*/
  scriptPublicKey: ScriptPublicKey;
/**
*/
  value: bigint;
}
/**
* @category Wallet SDK
*/
export class TransactionRecord {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Check if the transaction record has the given address within the associated UTXO set.
* @param {Address} address
* @returns {boolean}
*/
  hasAddress(address: Address): boolean;
/**
* Serialize the transaction record to a JavaScript object.
* @returns {any}
*/
  serialize(): any;
/**
*/
  readonly binding: IBinding;
/**
*/
  readonly blockDaaScore: bigint;
/**
*/
  readonly data: ITransactionData;
/**
*/
  id: Hash;
/**
*/
  metadata?: string;
/**
*/
  network: NetworkId;
/**
*/
  note?: string;
/**
*/
  readonly type: string;
/**
* Unix time in milliseconds
*/
  unixtimeMsec?: bigint;
/**
*/
  readonly value: bigint;
}
/**
*/
export class TransactionRecordNotification {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  data: TransactionRecord;
/**
*/
  type: string;
}
/**
* @category Wallet SDK
*/
export class TransactionSigningHash {
  free(): void;
/**
*/
  constructor();
/**
* @param {HexString | Uint8Array} data
*/
  update(data: HexString | Uint8Array): void;
/**
* @returns {string}
*/
  finalize(): string;
}
/**
* @category Wallet SDK
*/
export class TransactionSigningHashECDSA {
  free(): void;
/**
*/
  constructor();
/**
* @param {HexString | Uint8Array} data
*/
  update(data: HexString | Uint8Array): void;
/**
* @returns {string}
*/
  finalize(): string;
}
/**
* Holds details about an individual transaction output in a utxo
* set such as whether or not it was contained in a coinbase tx, the daa
* score of the block that accepts the tx, its public key script, and how
* much it pays.
* @category Consensus
*/
export class TransactionUtxoEntry {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  amount: bigint;
/**
*/
  blockDaaScore: bigint;
/**
*/
  isCoinbase: boolean;
/**
*/
  scriptPublicKey: ScriptPublicKey;
}
/**
*/
export class UserInfoOptions {
  free(): void;
/**
* @param {string | undefined} [encoding]
*/
  constructor(encoding?: string);
/**
* @returns {UserInfoOptions}
*/
  static new(): UserInfoOptions;
/**
*/
  encoding?: string;
}
/**
*
* UtxoContext is a class that provides a way to track addresses activity
* on the Kaspa network.  When an address is registered with UtxoContext
* it aggregates all UTXO entries for that address and emits events when
* any activity against these addresses occurs.
*
* UtxoContext constructor accepts {@link IUtxoContextArgs} interface that
* can contain an optional id parameter.  If supplied, this `id` parameter
* will be included in all notifications emitted by the UtxoContext as
* well as included as a part of {@link ITransactionRecord} emitted when
* transactions occur. If not provided, a random id will be generated. This id
* typically represents an account id in the context of a wallet application.
* The integrated Wallet API uses UtxoContext to represent wallet accounts.
*
* **Exchanges:** if you are building an exchange wallet, it is recommended
* to use UtxoContext for each user account.  This way you can track and isolate
* each user activity (use address set, balances, transaction records).
*
* UtxoContext maintains a real-time cumulative balance of all addresses
* registered against it and provides balance update notification events
* when the balance changes.
*
* The UtxoContext balance is comprised of 3 values:
* - `mature`: amount of funds available for spending.
* - `pending`: amount of funds that are being received.
* - `outgoing`: amount of funds that are being sent but are not yet accepted by the network.
*
* Please see {@link IBalance} interface for more details.
*
* UtxoContext can be supplied as a UTXO source to the transaction {@link Generator}
* allowing the {@link Generator} to create transactions using the
* UTXO entries it manages.
*
* **IMPORTANT:** UtxoContext is meant to represent a single account.  It is not
* designed to be used as a global UTXO manager for all addresses in a very large
* wallet (such as an exchange wallet). For such use cases, it is recommended to
* perform manual UTXO management by subscribing to UTXO notifications using
* {@link RpcClient.subscribeUtxosChanged} and {@link RpcClient.getUtxosByAddresses}.
*
* @see {@link IUtxoContextArgs},
* {@link UtxoProcessor},
* {@link Generator},
* {@link createTransactions},
* {@link IBalance},
* {@link IBalanceEvent},
* {@link IPendingEvent},
* {@link IReorgEvent},
* {@link IStasisEvent},
* {@link IMaturityEvent},
* {@link IDiscoveryEvent},
* {@link IBalanceEvent},
* {@link ITransactionRecord}
*
* @category Wallet SDK
*/
export class UtxoContext {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {IUtxoContextArgs} js_value
*/
  constructor(js_value: IUtxoContextArgs);
/**
* Performs a scan of the given addresses and registers them in the context for event notifications.
* @param {(Address | string)[]} addresses
* @param {bigint | undefined} [optional_current_daa_score]
* @returns {Promise<void>}
*/
  trackAddresses(addresses: (Address | string)[], optional_current_daa_score?: bigint): Promise<void>;
/**
* Unregister a list of addresses from the context. This will stop tracking of these addresses.
* @param {(Address | string)[]} addresses
* @returns {Promise<void>}
*/
  unregisterAddresses(addresses: (Address | string)[]): Promise<void>;
/**
* Clear the UtxoContext.  Unregister all addresses and clear all UTXO entries.
* IMPORTANT: This function must be manually called when disconnecting or re-connecting to the node
* (followed by address re-registration).  
* @returns {Promise<void>}
*/
  clear(): Promise<void>;
/**
*
* Returns a range of mature UTXO entries that are currently
* managed by the UtxoContext and are available for spending.
*
* NOTE: This function is provided for informational purposes only.
* **You should not manage UTXO entries manually if they are owned by UtxoContext.**
*
* The resulting range may be less than requested if UTXO entries
* have been spent asynchronously by UtxoContext or by other means
* (i.e. UtxoContext has received notification from the network that
* UtxoEntries have been spent externally).
*
* UtxoEntries are kept in in the ascending sorted order by their amount.
* @param {number} from
* @param {number} to
* @returns {UtxoEntryReference[]}
*/
  getMatureRange(from: number, to: number): UtxoEntryReference[];
/**
* Returns pending UTXO entries that are currently managed by the UtxoContext.
* @returns {UtxoEntryReference[]}
*/
  getPending(): UtxoEntryReference[];
/**
* Current {@link Balance} of the UtxoContext.
*/
  readonly balance: Balance | undefined;
/**
* Current {@link BalanceStrings} of the UtxoContext.
*/
  readonly balanceStrings: BalanceStrings | undefined;
/**
*/
  readonly isActive: boolean;
/**
* Obtain the length of the mature UTXO entries that are currently
* managed by the UtxoContext.
*/
  readonly matureLength: number;
}
/**
* A simple collection of UTXO entries. This struct is used to
* retain a set of UTXO entries in the WASM memory for faster
* processing. This struct keeps a list of entries represented
* by `UtxoEntryReference` struct. This data structure is used
* internally by the framework, but is exposed for convenience.
* Please consider using `UtxoContext` instead.
* @category Wallet SDK
*/
export class UtxoEntries {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* Create a new `UtxoEntries` struct with a set of entries.
* @param {any} js_value
*/
  constructor(js_value: any);
/**
* Sort the contained entries by amount. Please note that
* this function is not intended for use with large UTXO sets
* as it duplicates the whole contained UTXO set while sorting.
*/
  sort(): void;
/**
* @returns {bigint}
*/
  amount(): bigint;
/**
*/
  items: any;
}
/**
* @category Wallet SDK
*/
export class UtxoEntry {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @returns {string}
*/
  toString(): string;
/**
*/
  address?: Address;
/**
*/
  amount: bigint;
/**
*/
  blockDaaScore: bigint;
/**
*/
  isCoinbase: boolean;
/**
*/
  outpoint: TransactionOutpoint;
/**
*/
  scriptPublicKey: ScriptPublicKey;
}
/**
* @category Wallet SDK
*/
export class UtxoEntryReference {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @returns {string}
*/
  toString(): string;
/**
*/
  readonly address: Address | undefined;
/**
*/
  readonly amount: bigint;
/**
*/
  readonly blockDaaScore: bigint;
/**
*/
  readonly entry: UtxoEntry;
/**
*/
  readonly isCoinbase: boolean;
/**
*/
  readonly outpoint: TransactionOutpoint;
/**
*/
  readonly scriptPublicKey: ScriptPublicKey;
}
/**
*
* UtxoProcessor class is the main coordinator that manages UTXO processing
* between multiple UtxoContext instances. It acts as a bridge between the
* Kaspa node RPC connection, address subscriptions and UtxoContext instances.
*
* @see {@link IUtxoProcessorArgs},
* {@link UtxoContext},
* {@link RpcClient},
* {@link NetworkId},
* {@link IConnectEvent}
* {@link IDisconnectEvent}
* @category Wallet SDK
*/
export class UtxoProcessor {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {UtxoProcessorEventType | UtxoProcessorEventType[] | string | string[]} event
* @param {UtxoProcessorNotificationCallback | undefined} [callback]
*/
  removeEventListener(event: UtxoProcessorEventType | UtxoProcessorEventType[] | string | string[], callback?: UtxoProcessorNotificationCallback): void;
/**
* UtxoProcessor constructor.
*
*
*
* @see {@link IUtxoProcessorArgs}
* @param {IUtxoProcessorArgs} js_value
*/
  constructor(js_value: IUtxoProcessorArgs);
/**
* Starts the UtxoProcessor and begins processing UTXO and other notifications.
* @returns {Promise<void>}
*/
  start(): Promise<void>;
/**
* Stops the UtxoProcessor and ends processing UTXO and other notifications.
* @returns {Promise<void>}
*/
  stop(): Promise<void>;
/**
* @param {NetworkId | string} network_id
*/
  setNetworkId(network_id: NetworkId | string): void;
/**
*
* Set the coinbase transaction maturity period DAA score for a given network.
* This controls the DAA period after which the user transactions are considered mature
* and the wallet subsystem emits the transaction maturity event.
*
* @see {@link TransactionRecord}
* @see {@link IUtxoProcessorEvent}
*
* @category Wallet SDK
* @param {NetworkId | string} network_id
* @param {bigint} value
*/
  static setCoinbaseTransactionMaturityDAA(network_id: NetworkId | string, value: bigint): void;
/**
*
* Set the user transaction maturity period DAA score for a given network.
* This controls the DAA period after which the user transactions are considered mature
* and the wallet subsystem emits the transaction maturity event.
*
* @see {@link TransactionRecord}
* @see {@link IUtxoProcessorEvent}
*
* @category Wallet SDK
* @param {NetworkId | string} network_id
* @param {bigint} value
*/
  static setUserTransactionMaturityDAA(network_id: NetworkId | string, value: bigint): void;
/**
*/
  readonly isActive: boolean;
/**
*/
  readonly networkId: string | undefined;
/**
*/
  readonly rpc: RpcClient;
}
/**
*
* Wallet class is the main coordinator that manages integrated wallet operations.
*
* The Wallet class encapsulates {@link UtxoProcessor} and provides internal
* account management using {@link UtxoContext} instances. It acts as a bridge
* between the integrated Wallet subsystem providing a high-level interface
* for wallet key and account management.
*
* The Rusty Kaspa is developed in Rust, and the Wallet class is a Rust implementation
* exposed to the JavaScript/TypeScript environment using the WebAssembly (WASM32) interface.
* As such, the Wallet implementation can be powered up using native Rust or built
* as a WebAssembly module and used in the browser or Node.js environment.
*
* When using Rust native or NodeJS environment, all wallet data is stored on the local
* filesystem.  When using WASM32 build in the web browser, the wallet data is stored
* in the browser's `localStorage` and transaction records are stored in the `IndexedDB`.
*
* The Wallet API can create multiple wallet instances, however, only one wallet instance
* can be active at a time.
*
* The wallet implementation is designed to be efficient and support a large number
* of accounts. Accounts reside in storage and can be loaded and activated as needed.
* A `loaded` account contains all account information loaded from the permanent storage
* whereas an `active` account monitors the UTXO set and provides notifications for
* incoming and outgoing transactions as well as balance updates.
*
* The Wallet API communicates with the client using resource identifiers. These include
* account IDs, private key IDs, transaction IDs, etc. It is the responsibility of the
* client to track these resource identifiers at runtime.
*
* @see {@link IWalletConfig},
*
* @category Wallet API
*/
export class Wallet {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {IWalletConfig} config
*/
  constructor(config: IWalletConfig);
/**
* Check if a wallet with a given name exists.
* @param {string | undefined} [name]
* @returns {Promise<boolean>}
*/
  exists(name?: string): Promise<boolean>;
/**
* @returns {Promise<void>}
*/
  start(): Promise<void>;
/**
* @returns {Promise<void>}
*/
  stop(): Promise<void>;
/**
* @param {IConnectOptions | undefined | undefined} [args]
* @returns {Promise<void>}
*/
  connect(args?: IConnectOptions | undefined): Promise<void>;
/**
* @returns {Promise<void>}
*/
  disconnect(): Promise<void>;
/**
* @param {WalletEventType | WalletEventType[] | string | string[]} event
* @param {WalletNotificationCallback | undefined} [callback]
*/
  removeEventListener(event: WalletEventType | WalletEventType[] | string | string[], callback?: WalletNotificationCallback): void;
/**
* Ping backend
*@see {@link IBatchRequest} {@link IBatchResponse}
*@throws `string` in case of an error.
* @param {IBatchRequest} request
* @returns {Promise<IBatchResponse>}
*/
  batch(request: IBatchRequest): Promise<IBatchResponse>;
/**
*@see {@link IFlushRequest} {@link IFlushResponse}
*@throws `string` in case of an error.
* @param {IFlushRequest} request
* @returns {Promise<IFlushResponse>}
*/
  flush(request: IFlushRequest): Promise<IFlushResponse>;
/**
*@see {@link IRetainContextRequest} {@link IRetainContextResponse}
*@throws `string` in case of an error.
* @param {IRetainContextRequest} request
* @returns {Promise<IRetainContextResponse>}
*/
  retainContext(request: IRetainContextRequest): Promise<IRetainContextResponse>;
/**
*@see {@link IGetStatusRequest} {@link IGetStatusResponse}
*@throws `string` in case of an error.
* @param {IGetStatusRequest} request
* @returns {Promise<IGetStatusResponse>}
*/
  getStatus(request: IGetStatusRequest): Promise<IGetStatusResponse>;
/**
*@see {@link IWalletEnumerateRequest} {@link IWalletEnumerateResponse}
*@throws `string` in case of an error.
* @param {IWalletEnumerateRequest} request
* @returns {Promise<IWalletEnumerateResponse>}
*/
  walletEnumerate(request: IWalletEnumerateRequest): Promise<IWalletEnumerateResponse>;
/**
*@see {@link IWalletCreateRequest} {@link IWalletCreateResponse}
*@throws `string` in case of an error.
* @param {IWalletCreateRequest} request
* @returns {Promise<IWalletCreateResponse>}
*/
  walletCreate(request: IWalletCreateRequest): Promise<IWalletCreateResponse>;
/**
*@see {@link IWalletOpenRequest} {@link IWalletOpenResponse}
*@throws `string` in case of an error.
* @param {IWalletOpenRequest} request
* @returns {Promise<IWalletOpenResponse>}
*/
  walletOpen(request: IWalletOpenRequest): Promise<IWalletOpenResponse>;
/**
*@see {@link IWalletReloadRequest} {@link IWalletReloadResponse}
*@throws `string` in case of an error.
* @param {IWalletReloadRequest} request
* @returns {Promise<IWalletReloadResponse>}
*/
  walletReload(request: IWalletReloadRequest): Promise<IWalletReloadResponse>;
/**
*@see {@link IWalletCloseRequest} {@link IWalletCloseResponse}
*@throws `string` in case of an error.
* @param {IWalletCloseRequest} request
* @returns {Promise<IWalletCloseResponse>}
*/
  walletClose(request: IWalletCloseRequest): Promise<IWalletCloseResponse>;
/**
*@see {@link IWalletChangeSecretRequest} {@link IWalletChangeSecretResponse}
*@throws `string` in case of an error.
* @param {IWalletChangeSecretRequest} request
* @returns {Promise<IWalletChangeSecretResponse>}
*/
  walletChangeSecret(request: IWalletChangeSecretRequest): Promise<IWalletChangeSecretResponse>;
/**
*@see {@link IWalletExportRequest} {@link IWalletExportResponse}
*@throws `string` in case of an error.
* @param {IWalletExportRequest} request
* @returns {Promise<IWalletExportResponse>}
*/
  walletExport(request: IWalletExportRequest): Promise<IWalletExportResponse>;
/**
*@see {@link IWalletImportRequest} {@link IWalletImportResponse}
*@throws `string` in case of an error.
* @param {IWalletImportRequest} request
* @returns {Promise<IWalletImportResponse>}
*/
  walletImport(request: IWalletImportRequest): Promise<IWalletImportResponse>;
/**
*@see {@link IPrvKeyDataEnumerateRequest} {@link IPrvKeyDataEnumerateResponse}
*@throws `string` in case of an error.
* @param {IPrvKeyDataEnumerateRequest} request
* @returns {Promise<IPrvKeyDataEnumerateResponse>}
*/
  prvKeyDataEnumerate(request: IPrvKeyDataEnumerateRequest): Promise<IPrvKeyDataEnumerateResponse>;
/**
*@see {@link IPrvKeyDataCreateRequest} {@link IPrvKeyDataCreateResponse}
*@throws `string` in case of an error.
* @param {IPrvKeyDataCreateRequest} request
* @returns {Promise<IPrvKeyDataCreateResponse>}
*/
  prvKeyDataCreate(request: IPrvKeyDataCreateRequest): Promise<IPrvKeyDataCreateResponse>;
/**
*@see {@link IPrvKeyDataRemoveRequest} {@link IPrvKeyDataRemoveResponse}
*@throws `string` in case of an error.
* @param {IPrvKeyDataRemoveRequest} request
* @returns {Promise<IPrvKeyDataRemoveResponse>}
*/
  prvKeyDataRemove(request: IPrvKeyDataRemoveRequest): Promise<IPrvKeyDataRemoveResponse>;
/**
*@see {@link IPrvKeyDataGetRequest} {@link IPrvKeyDataGetResponse}
*@throws `string` in case of an error.
* @param {IPrvKeyDataGetRequest} request
* @returns {Promise<IPrvKeyDataGetResponse>}
*/
  prvKeyDataGet(request: IPrvKeyDataGetRequest): Promise<IPrvKeyDataGetResponse>;
/**
*@see {@link IAccountsEnumerateRequest} {@link IAccountsEnumerateResponse}
*@throws `string` in case of an error.
* @param {IAccountsEnumerateRequest} request
* @returns {Promise<IAccountsEnumerateResponse>}
*/
  accountsEnumerate(request: IAccountsEnumerateRequest): Promise<IAccountsEnumerateResponse>;
/**
*@see {@link IAccountsRenameRequest} {@link IAccountsRenameResponse}
*@throws `string` in case of an error.
* @param {IAccountsRenameRequest} request
* @returns {Promise<IAccountsRenameResponse>}
*/
  accountsRename(request: IAccountsRenameRequest): Promise<IAccountsRenameResponse>;
/**
*@see {@link IAccountsDiscoveryRequest} {@link IAccountsDiscoveryResponse}
*@throws `string` in case of an error.
* @param {IAccountsDiscoveryRequest} request
* @returns {Promise<IAccountsDiscoveryResponse>}
*/
  accountsDiscovery(request: IAccountsDiscoveryRequest): Promise<IAccountsDiscoveryResponse>;
/**
*@see {@link IAccountsCreateRequest} {@link IAccountsCreateResponse}
*@throws `string` in case of an error.
* @param {IAccountsCreateRequest} request
* @returns {Promise<IAccountsCreateResponse>}
*/
  accountsCreate(request: IAccountsCreateRequest): Promise<IAccountsCreateResponse>;
/**
*@see {@link IAccountsEnsureDefaultRequest} {@link IAccountsEnsureDefaultResponse}
*@throws `string` in case of an error.
* @param {IAccountsEnsureDefaultRequest} request
* @returns {Promise<IAccountsEnsureDefaultResponse>}
*/
  accountsEnsureDefault(request: IAccountsEnsureDefaultRequest): Promise<IAccountsEnsureDefaultResponse>;
/**
*@see {@link IAccountsImportRequest} {@link IAccountsImportResponse}
*@throws `string` in case of an error.
* @param {IAccountsImportRequest} request
* @returns {Promise<IAccountsImportResponse>}
*/
  accountsImport(request: IAccountsImportRequest): Promise<IAccountsImportResponse>;
/**
*@see {@link IAccountsActivateRequest} {@link IAccountsActivateResponse}
*@throws `string` in case of an error.
* @param {IAccountsActivateRequest} request
* @returns {Promise<IAccountsActivateResponse>}
*/
  accountsActivate(request: IAccountsActivateRequest): Promise<IAccountsActivateResponse>;
/**
*@see {@link IAccountsDeactivateRequest} {@link IAccountsDeactivateResponse}
*@throws `string` in case of an error.
* @param {IAccountsDeactivateRequest} request
* @returns {Promise<IAccountsDeactivateResponse>}
*/
  accountsDeactivate(request: IAccountsDeactivateRequest): Promise<IAccountsDeactivateResponse>;
/**
*@see {@link IAccountsGetRequest} {@link IAccountsGetResponse}
*@throws `string` in case of an error.
* @param {IAccountsGetRequest} request
* @returns {Promise<IAccountsGetResponse>}
*/
  accountsGet(request: IAccountsGetRequest): Promise<IAccountsGetResponse>;
/**
*@see {@link IAccountsCreateNewAddressRequest} {@link IAccountsCreateNewAddressResponse}
*@throws `string` in case of an error.
* @param {IAccountsCreateNewAddressRequest} request
* @returns {Promise<IAccountsCreateNewAddressResponse>}
*/
  accountsCreateNewAddress(request: IAccountsCreateNewAddressRequest): Promise<IAccountsCreateNewAddressResponse>;
/**
*@see {@link IAccountsSendRequest} {@link IAccountsSendResponse}
*@throws `string` in case of an error.
* @param {IAccountsSendRequest} request
* @returns {Promise<IAccountsSendResponse>}
*/
  accountsSend(request: IAccountsSendRequest): Promise<IAccountsSendResponse>;
/**
*@see {@link IAccountsTransferRequest} {@link IAccountsTransferResponse}
*@throws `string` in case of an error.
* @param {IAccountsTransferRequest} request
* @returns {Promise<IAccountsTransferResponse>}
*/
  accountsTransfer(request: IAccountsTransferRequest): Promise<IAccountsTransferResponse>;
/**
*@see {@link IAccountsEstimateRequest} {@link IAccountsEstimateResponse}
*@throws `string` in case of an error.
* @param {IAccountsEstimateRequest} request
* @returns {Promise<IAccountsEstimateResponse>}
*/
  accountsEstimate(request: IAccountsEstimateRequest): Promise<IAccountsEstimateResponse>;
/**
*@see {@link ITransactionsDataGetRequest} {@link ITransactionsDataGetResponse}
*@throws `string` in case of an error.
* @param {ITransactionsDataGetRequest} request
* @returns {Promise<ITransactionsDataGetResponse>}
*/
  transactionsDataGet(request: ITransactionsDataGetRequest): Promise<ITransactionsDataGetResponse>;
/**
*@see {@link ITransactionsReplaceNoteRequest} {@link ITransactionsReplaceNoteResponse}
*@throws `string` in case of an error.
* @param {ITransactionsReplaceNoteRequest} request
* @returns {Promise<ITransactionsReplaceNoteResponse>}
*/
  transactionsReplaceNote(request: ITransactionsReplaceNoteRequest): Promise<ITransactionsReplaceNoteResponse>;
/**
*@see {@link ITransactionsReplaceMetadataRequest} {@link ITransactionsReplaceMetadataResponse}
*@throws `string` in case of an error.
* @param {ITransactionsReplaceMetadataRequest} request
* @returns {Promise<ITransactionsReplaceMetadataResponse>}
*/
  transactionsReplaceMetadata(request: ITransactionsReplaceMetadataRequest): Promise<ITransactionsReplaceMetadataResponse>;
/**
*@see {@link IAddressBookEnumerateRequest} {@link IAddressBookEnumerateResponse}
*@throws `string` in case of an error.
* @param {IAddressBookEnumerateRequest} request
* @returns {Promise<IAddressBookEnumerateResponse>}
*/
  addressBookEnumerate(request: IAddressBookEnumerateRequest): Promise<IAddressBookEnumerateResponse>;
/**
*/
  readonly descriptor: WalletDescriptor | undefined;
/**
* @remarks This is a local property indicating
* if the wallet is currently open.
*/
  readonly isOpen: boolean;
/**
* @remarks This is a local property indicating
* if the node is currently synced.
*/
  readonly isSynced: boolean;
/**
*/
  readonly rpc: RpcClient;
}
/**
* @category Wallet API
*/
export class WalletDescriptor {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
*/
  filename: string;
/**
*/
  title?: string;
}
/**
*/
export class WasiOptions {
  free(): void;
/**
* @param {any[] | undefined} args
* @param {object | undefined} env
* @param {object} preopens
*/
  constructor(args: any[] | undefined, env: object | undefined, preopens: object);
/**
* @param {object} preopens
* @returns {WasiOptions}
*/
  static new(preopens: object): WasiOptions;
/**
*/
  args?: any[];
/**
*/
  env?: object;
/**
*/
  preopens: object;
}
/**
*/
export class WriteFileSyncOptions {
  free(): void;
/**
* @param {string | undefined} [encoding]
* @param {string | undefined} [flag]
* @param {number | undefined} [mode]
*/
  constructor(encoding?: string, flag?: string, mode?: number);
/**
*/
  encoding?: string;
/**
*/
  flag?: string;
/**
*/
  mode?: number;
}
/**
*
* Data structure that envelopes a XOnlyPublicKey.
*
* XOnlyPublicKey is used as a payload part of the {@link Address}.
*
* @see {@link PublicKey}
* @category Wallet SDK
*/
export class XOnlyPublicKey {
  free(): void;
/**
* @param {string} key
*/
  constructor(key: string);
/**
* @returns {string}
*/
  toString(): string;
/**
* Get the [`Address`] of this XOnlyPublicKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = xOnlyPublicKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddress(network: NetworkType | NetworkId | string): Address;
/**
* Get `ECDSA` [`Address`] of this XOnlyPublicKey.
* Receives a [`NetworkType`] to determine the prefix of the address.
* JavaScript: `let address = xOnlyPublicKey.toAddress(NetworkType.MAINNET);`.
* @param {NetworkType | NetworkId | string} network
* @returns {Address}
*/
  toAddressECDSA(network: NetworkType | NetworkId | string): Address;
/**
* @param {Address} address
* @returns {XOnlyPublicKey}
*/
  static fromAddress(address: Address): XOnlyPublicKey;
}
/**
*
* Extended private key (XPrv).
*
* This class allows accepts a master seed and provides
* functions for derivation of dependent child private keys.
*
* Please note that Kaspa extended private keys use `kprv` prefix.
*
* @see {@link PrivateKeyGenerator}, {@link PublicKeyGenerator}, {@link XPub}, {@link Mnemonic}
* @category Wallet SDK
*/
export class XPrv {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {HexString} seed
*/
  constructor(seed: HexString);
/**
* Create {@link XPrv} from `xprvxxxx..` string
* @param {string} xprv
* @returns {XPrv}
*/
  static fromXPrv(xprv: string): XPrv;
/**
* @param {number} child_number
* @param {boolean | undefined} [hardened]
* @returns {XPrv}
*/
  deriveChild(child_number: number, hardened?: boolean): XPrv;
/**
* @param {any} path
* @returns {XPrv}
*/
  derivePath(path: any): XPrv;
/**
* @param {string} prefix
* @returns {string}
*/
  intoString(prefix: string): string;
/**
* @returns {string}
*/
  toString(): string;
/**
* @returns {XPub}
*/
  toXPub(): XPub;
/**
* @returns {PrivateKey}
*/
  toPrivateKey(): PrivateKey;
/**
*/
  readonly chainCode: string;
/**
*/
  readonly childNumber: number;
/**
*/
  readonly depth: number;
/**
*/
  readonly parentFingerprint: string;
/**
*/
  readonly privateKey: string;
/**
*/
  readonly xprv: string;
}
/**
*
* Extended public key (XPub).
*
* This class allows accepts another XPub and and provides
* functions for derivation of dependent child public keys.
*
* Please note that Kaspa extended public keys use `kpub` prefix.
*
* @see {@link PrivateKeyGenerator}, {@link PublicKeyGenerator}, {@link XPrv}, {@link Mnemonic}
* @category Wallet SDK
*/
export class XPub {
/**
** Return copy of self without private attributes.
*/
  toJSON(): Object;
/**
* Return stringified version of self.
*/
  toString(): string;
  free(): void;
/**
* @param {string} xpub
*/
  constructor(xpub: string);
/**
* @param {number} child_number
* @param {boolean | undefined} [hardened]
* @returns {XPub}
*/
  deriveChild(child_number: number, hardened?: boolean): XPub;
/**
* @param {any} path
* @returns {XPub}
*/
  derivePath(path: any): XPub;
/**
* @param {string} prefix
* @returns {string}
*/
  intoString(prefix: string): string;
/**
* @returns {PublicKey}
*/
  toPublicKey(): PublicKey;
/**
*/
  readonly chainCode: string;
/**
*/
  readonly childNumber: number;
/**
*/
  readonly depth: number;
/**
*/
  readonly parentFingerprint: string;
/**
*/
  readonly xpub: string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_address_free: (a: number, b: number) => void;
  readonly address_constructor: (a: number, b: number) => number;
  readonly address_validate: (a: number, b: number) => number;
  readonly address_toString: (a: number, b: number) => void;
  readonly address_version: (a: number, b: number) => void;
  readonly address_prefix: (a: number, b: number) => void;
  readonly address_set_setPrefix: (a: number, b: number, c: number) => void;
  readonly address_payload: (a: number, b: number) => void;
  readonly address_short: (a: number, b: number, c: number) => void;
  readonly __wbg_mnemonic_free: (a: number, b: number) => void;
  readonly mnemonic_constructor: (a: number, b: number, c: number, d: number) => void;
  readonly mnemonic_validate: (a: number, b: number, c: number) => number;
  readonly mnemonic_entropy: (a: number, b: number) => void;
  readonly mnemonic_set_entropy: (a: number, b: number, c: number) => void;
  readonly mnemonic_random: (a: number, b: number, c: number) => void;
  readonly mnemonic_phrase: (a: number, b: number) => void;
  readonly mnemonic_set_phrase: (a: number, b: number, c: number) => void;
  readonly mnemonic_toSeed: (a: number, b: number, c: number, d: number) => void;
  readonly __wbg_utxoentry_free: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_address: (a: number) => number;
  readonly __wbg_set_utxoentry_address: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_outpoint: (a: number) => number;
  readonly __wbg_set_utxoentry_outpoint: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_amount: (a: number) => number;
  readonly __wbg_set_utxoentry_amount: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_scriptPublicKey: (a: number) => number;
  readonly __wbg_set_utxoentry_scriptPublicKey: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_blockDaaScore: (a: number) => number;
  readonly __wbg_set_utxoentry_blockDaaScore: (a: number, b: number) => void;
  readonly __wbg_get_utxoentry_isCoinbase: (a: number) => number;
  readonly __wbg_set_utxoentry_isCoinbase: (a: number, b: number) => void;
  readonly utxoentry_toString: (a: number, b: number) => void;
  readonly __wbg_utxoentryreference_free: (a: number, b: number) => void;
  readonly utxoentryreference_toString: (a: number, b: number) => void;
  readonly utxoentryreference_entry: (a: number) => number;
  readonly utxoentryreference_outpoint: (a: number) => number;
  readonly utxoentryreference_address: (a: number) => number;
  readonly utxoentryreference_amount: (a: number) => number;
  readonly utxoentryreference_isCoinbase: (a: number) => number;
  readonly utxoentryreference_blockDaaScore: (a: number) => number;
  readonly utxoentryreference_scriptPublicKey: (a: number) => number;
  readonly __wbg_utxoentries_free: (a: number, b: number) => void;
  readonly utxoentries_js_ctor: (a: number, b: number) => void;
  readonly utxoentries_get_items_as_js_array: (a: number) => number;
  readonly utxoentries_set_items_from_js_array: (a: number, b: number) => void;
  readonly utxoentries_sort: (a: number) => void;
  readonly utxoentries_amount: (a: number) => number;
  readonly __wbg_transactionoutput_free: (a: number, b: number) => void;
  readonly transactionoutput_ctor: (a: number, b: number) => number;
  readonly transactionoutput_value: (a: number) => number;
  readonly transactionoutput_set_value: (a: number, b: number) => void;
  readonly transactionoutput_scriptPublicKey: (a: number) => number;
  readonly transactionoutput_set_scriptPublicKey: (a: number, b: number) => void;
  readonly __wbg_transaction_free: (a: number, b: number) => void;
  readonly transaction_is_coinbase: (a: number) => number;
  readonly transaction_finalize: (a: number, b: number) => void;
  readonly transaction_id: (a: number, b: number) => void;
  readonly transaction_constructor: (a: number, b: number) => void;
  readonly transaction_get_inputs_as_js_array: (a: number) => number;
  readonly transaction_addresses: (a: number, b: number, c: number) => void;
  readonly transaction_set_inputs_from_js_array: (a: number, b: number) => void;
  readonly transaction_get_outputs_as_js_array: (a: number) => number;
  readonly transaction_set_outputs_from_js_array: (a: number, b: number) => void;
  readonly transaction_version: (a: number) => number;
  readonly transaction_set_version: (a: number, b: number) => void;
  readonly transaction_lockTime: (a: number) => number;
  readonly transaction_set_lockTime: (a: number, b: number) => void;
  readonly transaction_gas: (a: number) => number;
  readonly transaction_set_gas: (a: number, b: number) => void;
  readonly transaction_get_subnetwork_id_as_hex: (a: number, b: number) => void;
  readonly transaction_set_subnetwork_id_from_js_value: (a: number, b: number) => void;
  readonly transaction_get_payload_as_hex_string: (a: number, b: number) => void;
  readonly transaction_set_payload_from_js_value: (a: number, b: number) => void;
  readonly transaction_get_mass: (a: number) => number;
  readonly transaction_set_mass: (a: number, b: number) => void;
  readonly transaction_serializeToObject: (a: number, b: number) => void;
  readonly transaction_serializeToJSON: (a: number, b: number) => void;
  readonly transaction_serializeToSafeJSON: (a: number, b: number) => void;
  readonly transaction_deserializeFromObject: (a: number, b: number) => void;
  readonly transaction_deserializeFromJSON: (a: number, b: number, c: number) => void;
  readonly transaction_deserializeFromSafeJSON: (a: number, b: number, c: number) => void;
  readonly transactionsigninghashecdsa_new: () => number;
  readonly transactionsigninghashecdsa_update: (a: number, b: number, c: number) => void;
  readonly transactionsigninghashecdsa_finalize: (a: number, b: number) => void;
  readonly __wbg_transactionsigninghashecdsa_free: (a: number, b: number) => void;
  readonly transactionsigninghash_new: () => number;
  readonly transactionsigninghash_update: (a: number, b: number, c: number) => void;
  readonly transactionsigninghash_finalize: (a: number, b: number) => void;
  readonly __wbg_transactionsigninghash_free: (a: number, b: number) => void;
  readonly __wbg_transactioninput_free: (a: number, b: number) => void;
  readonly transactioninput_constructor: (a: number, b: number) => void;
  readonly transactioninput_get_previous_outpoint: (a: number) => number;
  readonly transactioninput_set_previous_outpoint: (a: number, b: number, c: number) => void;
  readonly transactioninput_get_signature_script_as_hex: (a: number, b: number) => void;
  readonly transactioninput_set_signature_script_from_js_value: (a: number, b: number, c: number) => void;
  readonly transactioninput_get_sequence: (a: number) => number;
  readonly transactioninput_set_sequence: (a: number, b: number) => void;
  readonly transactioninput_get_sig_op_count: (a: number) => number;
  readonly transactioninput_set_sig_op_count: (a: number, b: number) => void;
  readonly transactioninput_get_utxo: (a: number) => number;
  readonly isScriptPayToScriptHash: (a: number, b: number) => void;
  readonly isScriptPayToPubkeyECDSA: (a: number, b: number) => void;
  readonly isScriptPayToPubkey: (a: number, b: number) => void;
  readonly addressFromScriptPublicKey: (a: number, b: number, c: number) => void;
  readonly payToScriptHashSignatureScript: (a: number, b: number, c: number) => void;
  readonly payToScriptHashScript: (a: number, b: number) => void;
  readonly payToAddressScript: (a: number, b: number) => void;
  readonly __wbg_transactionoutpoint_free: (a: number, b: number) => void;
  readonly transactionoutpoint_ctor: (a: number, b: number) => number;
  readonly transactionoutpoint_getId: (a: number, b: number) => void;
  readonly transactionoutpoint_transactionId: (a: number, b: number) => void;
  readonly transactionoutpoint_index: (a: number) => number;
  readonly header_constructor: (a: number, b: number) => void;
  readonly header_finalize: (a: number, b: number) => void;
  readonly header_asJSON: (a: number, b: number) => void;
  readonly header_get_version: (a: number) => number;
  readonly header_set_version: (a: number, b: number) => void;
  readonly header_get_timestamp: (a: number) => number;
  readonly header_set_timestamp: (a: number, b: number) => void;
  readonly header_bits: (a: number) => number;
  readonly header_set_bits: (a: number, b: number) => void;
  readonly header_nonce: (a: number) => number;
  readonly header_set_nonce: (a: number, b: number) => void;
  readonly header_daa_score: (a: number) => number;
  readonly header_set_daa_score: (a: number, b: number) => void;
  readonly header_blue_score: (a: number) => number;
  readonly header_set_blue_score: (a: number, b: number) => void;
  readonly header_get_hash_as_hex: (a: number, b: number) => void;
  readonly header_get_hash_merkle_root_as_hex: (a: number, b: number) => void;
  readonly header_set_hash_merkle_root_from_js_value: (a: number, b: number) => void;
  readonly header_get_accepted_id_merkle_root_as_hex: (a: number, b: number) => void;
  readonly header_set_accepted_id_merkle_root_from_js_value: (a: number, b: number) => void;
  readonly header_get_utxo_commitment_as_hex: (a: number, b: number) => void;
  readonly header_set_utxo_commitment_from_js_value: (a: number, b: number) => void;
  readonly header_get_pruning_point_as_hex: (a: number, b: number) => void;
  readonly header_set_pruning_point_from_js_value: (a: number, b: number) => void;
  readonly header_get_parents_by_level_as_js_value: (a: number) => number;
  readonly header_set_parents_by_level_from_js_value: (a: number, b: number) => void;
  readonly header_blue_work: (a: number) => number;
  readonly header_getBlueWorkAsHex: (a: number, b: number) => void;
  readonly header_set_blue_work_from_js_value: (a: number, b: number) => void;
  readonly __wbg_header_free: (a: number, b: number) => void;
  readonly __wbg_networkid_free: (a: number, b: number) => void;
  readonly __wbg_get_networkid_type: (a: number) => number;
  readonly __wbg_set_networkid_type: (a: number, b: number) => void;
  readonly __wbg_get_networkid_suffix: (a: number, b: number) => void;
  readonly __wbg_set_networkid_suffix: (a: number, b: number, c: number) => void;
  readonly networkid_ctor: (a: number, b: number) => void;
  readonly networkid_id: (a: number, b: number) => void;
  readonly networkid_addressPrefix: (a: number, b: number) => void;
  readonly networkid_toString: (a: number, b: number) => void;
  readonly __wbg_scriptpublickey_free: (a: number, b: number) => void;
  readonly __wbg_get_scriptpublickey_version: (a: number) => number;
  readonly __wbg_set_scriptpublickey_version: (a: number, b: number) => void;
  readonly scriptpublickey_constructor: (a: number, b: number, c: number) => void;
  readonly scriptpublickey_script_as_hex: (a: number, b: number) => void;
  readonly __wbg_sighashtype_free: (a: number, b: number) => void;
  readonly __wbg_transactionutxoentry_free: (a: number, b: number) => void;
  readonly __wbg_get_transactionutxoentry_amount: (a: number) => number;
  readonly __wbg_set_transactionutxoentry_amount: (a: number, b: number) => void;
  readonly __wbg_get_transactionutxoentry_scriptPublicKey: (a: number) => number;
  readonly __wbg_set_transactionutxoentry_scriptPublicKey: (a: number, b: number) => void;
  readonly __wbg_get_transactionutxoentry_blockDaaScore: (a: number) => number;
  readonly __wbg_set_transactionutxoentry_blockDaaScore: (a: number, b: number) => void;
  readonly __wbg_get_transactionutxoentry_isCoinbase: (a: number) => number;
  readonly __wbg_set_transactionutxoentry_isCoinbase: (a: number, b: number) => void;
  readonly __wbg_hash_free: (a: number, b: number) => void;
  readonly hash_constructor: (a: number, b: number) => number;
  readonly hash_toString: (a: number, b: number) => void;
  readonly __wbg_pow_free: (a: number, b: number) => void;
  readonly pow_new: (a: number, b: number, c: number, d: number) => void;
  readonly pow_target: (a: number, b: number) => void;
  readonly pow_checkWork: (a: number, b: number, c: number) => void;
  readonly pow_get_pre_pow_hash: (a: number, b: number) => void;
  readonly pow_fromRaw: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly calculateTarget: (a: number, b: number) => void;
  readonly scriptbuilder_new: () => number;
  readonly scriptbuilder_fromScript: (a: number, b: number) => void;
  readonly scriptbuilder_addOp: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_addOps: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_addData: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_addI64: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_addLockTime: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_canonicalDataSize: (a: number, b: number) => void;
  readonly scriptbuilder_toString: (a: number) => number;
  readonly scriptbuilder_drain: (a: number) => number;
  readonly scriptbuilder_createPayToScriptHashScript: (a: number) => number;
  readonly scriptbuilder_encodePayToScriptHashSignatureScript: (a: number, b: number, c: number) => void;
  readonly scriptbuilder_hexView: (a: number, b: number, c: number) => void;
  readonly __wbg_scriptbuilder_free: (a: number, b: number) => void;
  readonly scriptbuilder_addSequence: (a: number, b: number, c: number) => void;
  readonly wallet_constructor: (a: number, b: number) => void;
  readonly wallet_rpc: (a: number) => number;
  readonly wallet_isOpen: (a: number) => number;
  readonly wallet_isSynced: (a: number) => number;
  readonly wallet_descriptor: (a: number) => number;
  readonly wallet_exists: (a: number, b: number, c: number) => number;
  readonly wallet_start: (a: number) => number;
  readonly wallet_stop: (a: number) => number;
  readonly wallet_connect: (a: number, b: number) => number;
  readonly wallet_disconnect: (a: number) => number;
  readonly wallet_addEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly wallet_removeEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly __wbg_wallet_free: (a: number, b: number) => void;
  readonly utxoprocessor_addEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly utxoprocessor_removeEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly utxoprocessor_ctor: (a: number, b: number) => void;
  readonly utxoprocessor_start: (a: number) => number;
  readonly utxoprocessor_stop: (a: number) => number;
  readonly utxoprocessor_rpc: (a: number) => number;
  readonly utxoprocessor_networkId: (a: number, b: number) => void;
  readonly utxoprocessor_setNetworkId: (a: number, b: number, c: number) => void;
  readonly utxoprocessor_isActive: (a: number) => number;
  readonly utxoprocessor_setCoinbaseTransactionMaturityDAA: (a: number, b: number, c: number) => void;
  readonly utxoprocessor_setUserTransactionMaturityDAA: (a: number, b: number, c: number) => void;
  readonly __wbg_utxoprocessor_free: (a: number, b: number) => void;
  readonly generator_ctor: (a: number, b: number) => void;
  readonly generator_next: (a: number) => number;
  readonly generator_estimate: (a: number) => number;
  readonly generator_summary: (a: number) => number;
  readonly __wbg_generator_free: (a: number, b: number) => void;
  readonly cryptobox_ctor: (a: number, b: number, c: number) => void;
  readonly cryptobox_publicKey: (a: number, b: number) => void;
  readonly cryptobox_encrypt: (a: number, b: number, c: number, d: number) => void;
  readonly cryptobox_decrypt: (a: number, b: number, c: number, d: number) => void;
  readonly __wbg_cryptobox_free: (a: number, b: number) => void;
  readonly cryptoboxpublickey_ctor: (a: number, b: number) => void;
  readonly cryptoboxpublickey_toString: (a: number, b: number) => void;
  readonly __wbg_cryptoboxpublickey_free: (a: number, b: number) => void;
  readonly cryptoboxprivatekey_ctor: (a: number, b: number) => void;
  readonly cryptoboxprivatekey_to_public_key: (a: number) => number;
  readonly __wbg_cryptoboxprivatekey_free: (a: number, b: number) => void;
  readonly utxocontext_ctor: (a: number, b: number) => void;
  readonly utxocontext_trackAddresses: (a: number, b: number, c: number) => number;
  readonly utxocontext_unregisterAddresses: (a: number, b: number) => number;
  readonly utxocontext_clear: (a: number) => number;
  readonly utxocontext_isActive: (a: number) => number;
  readonly utxocontext_getMatureRange: (a: number, b: number, c: number, d: number) => void;
  readonly utxocontext_matureLength: (a: number) => number;
  readonly utxocontext_getPending: (a: number, b: number) => void;
  readonly utxocontext_balance: (a: number) => number;
  readonly utxocontext_balanceStrings: (a: number, b: number) => void;
  readonly __wbg_utxocontext_free: (a: number, b: number) => void;
  readonly estimateTransactions: (a: number) => number;
  readonly createTransactions: (a: number) => number;
  readonly createTransaction: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly __wbg_accountkind_free: (a: number, b: number) => void;
  readonly accountkind_ctor: (a: number, b: number, c: number) => void;
  readonly accountkind_toString: (a: number, b: number) => void;
  readonly createAddress: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly createMultisigAddress: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly __wbg_storage_free: (a: number, b: number) => void;
  readonly storage_filename: (a: number, b: number) => void;
  readonly __wbg_paymentoutput_free: (a: number, b: number) => void;
  readonly __wbg_get_paymentoutput_address: (a: number) => number;
  readonly __wbg_set_paymentoutput_address: (a: number, b: number) => void;
  readonly __wbg_get_paymentoutput_amount: (a: number) => number;
  readonly __wbg_set_paymentoutput_amount: (a: number, b: number) => void;
  readonly paymentoutput_new: (a: number, b: number) => number;
  readonly __wbg_paymentoutputs_free: (a: number, b: number) => void;
  readonly paymentoutputs_constructor: (a: number, b: number) => void;
  readonly generatorsummary_networkType: (a: number) => number;
  readonly generatorsummary_utxos: (a: number) => number;
  readonly generatorsummary_fees: (a: number) => number;
  readonly generatorsummary_mass: (a: number) => number;
  readonly generatorsummary_transactions: (a: number) => number;
  readonly generatorsummary_finalAmount: (a: number) => number;
  readonly generatorsummary_finalTransactionId: (a: number, b: number) => void;
  readonly __wbg_generatorsummary_free: (a: number, b: number) => void;
  readonly signScriptHash: (a: number, b: number, c: number) => void;
  readonly createInputSignature: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly signTransaction: (a: number, b: number, c: number, d: number) => void;
  readonly sompiToKaspaStringWithSuffix: (a: number, b: number, c: number) => void;
  readonly sompiToKaspaString: (a: number, b: number) => void;
  readonly kaspaToSompi: (a: number, b: number) => number;
  readonly pendingtransaction_id: (a: number, b: number) => void;
  readonly pendingtransaction_paymentAmount: (a: number) => number;
  readonly pendingtransaction_changeAmount: (a: number) => number;
  readonly pendingtransaction_feeAmount: (a: number) => number;
  readonly pendingtransaction_mass: (a: number) => number;
  readonly pendingtransaction_minimumSignatures: (a: number) => number;
  readonly pendingtransaction_aggregateInputAmount: (a: number) => number;
  readonly pendingtransaction_aggregateOutputAmount: (a: number) => number;
  readonly pendingtransaction_type: (a: number, b: number) => void;
  readonly pendingtransaction_addresses: (a: number) => number;
  readonly pendingtransaction_getUtxoEntries: (a: number) => number;
  readonly pendingtransaction_createInputSignature: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly pendingtransaction_fillInput: (a: number, b: number, c: number, d: number) => void;
  readonly pendingtransaction_signInput: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly pendingtransaction_sign: (a: number, b: number, c: number, d: number) => void;
  readonly pendingtransaction_submit: (a: number, b: number) => number;
  readonly pendingtransaction_transaction: (a: number, b: number) => void;
  readonly pendingtransaction_serializeToObject: (a: number, b: number) => void;
  readonly pendingtransaction_serializeToJSON: (a: number, b: number) => void;
  readonly pendingtransaction_serializeToSafeJSON: (a: number, b: number) => void;
  readonly __wbg_pendingtransaction_free: (a: number, b: number) => void;
  readonly setDefaultStorageFolder: (a: number, b: number, c: number) => void;
  readonly setDefaultWalletFile: (a: number, b: number, c: number) => void;
  readonly verifyMessage: (a: number, b: number) => void;
  readonly signMessage: (a: number, b: number) => void;
  readonly prvkeydatainfo_id: (a: number, b: number) => void;
  readonly prvkeydatainfo_name: (a: number) => number;
  readonly prvkeydatainfo_isEncrypted: (a: number) => number;
  readonly prvkeydatainfo_setName: (a: number, b: number, c: number, d: number) => void;
  readonly __wbg_prvkeydatainfo_free: (a: number, b: number) => void;
  readonly wallet_batch: (a: number, b: number) => number;
  readonly wallet_flush: (a: number, b: number) => number;
  readonly wallet_retainContext: (a: number, b: number) => number;
  readonly wallet_getStatus: (a: number, b: number) => number;
  readonly wallet_walletEnumerate: (a: number, b: number) => number;
  readonly wallet_walletCreate: (a: number, b: number) => number;
  readonly wallet_walletOpen: (a: number, b: number) => number;
  readonly wallet_walletReload: (a: number, b: number) => number;
  readonly wallet_walletClose: (a: number, b: number) => number;
  readonly wallet_walletChangeSecret: (a: number, b: number) => number;
  readonly wallet_walletExport: (a: number, b: number) => number;
  readonly wallet_walletImport: (a: number, b: number) => number;
  readonly wallet_prvKeyDataEnumerate: (a: number, b: number) => number;
  readonly wallet_prvKeyDataCreate: (a: number, b: number) => number;
  readonly wallet_prvKeyDataRemove: (a: number, b: number) => number;
  readonly wallet_prvKeyDataGet: (a: number, b: number) => number;
  readonly wallet_accountsEnumerate: (a: number, b: number) => number;
  readonly wallet_accountsRename: (a: number, b: number) => number;
  readonly wallet_accountsDiscovery: (a: number, b: number) => number;
  readonly wallet_accountsCreate: (a: number, b: number) => number;
  readonly wallet_accountsEnsureDefault: (a: number, b: number) => number;
  readonly wallet_accountsImport: (a: number, b: number) => number;
  readonly wallet_accountsActivate: (a: number, b: number) => number;
  readonly wallet_accountsDeactivate: (a: number, b: number) => number;
  readonly wallet_accountsGet: (a: number, b: number) => number;
  readonly wallet_accountsCreateNewAddress: (a: number, b: number) => number;
  readonly wallet_accountsSend: (a: number, b: number) => number;
  readonly wallet_accountsTransfer: (a: number, b: number) => number;
  readonly wallet_accountsEstimate: (a: number, b: number) => number;
  readonly wallet_transactionsDataGet: (a: number, b: number) => number;
  readonly wallet_transactionsReplaceNote: (a: number, b: number) => number;
  readonly wallet_transactionsReplaceMetadata: (a: number, b: number) => number;
  readonly wallet_addressBookEnumerate: (a: number, b: number) => number;
  readonly calculateStorageMass: (a: number, b: number, c: number, d: number) => void;
  readonly calculateTransactionFee: (a: number, b: number, c: number, d: number) => void;
  readonly updateTransactionMass: (a: number, b: number, c: number, d: number) => void;
  readonly calculateTransactionMass: (a: number, b: number, c: number, d: number) => void;
  readonly maximumStandardTransactionMass: () => number;
  readonly balancestrings_mature: (a: number, b: number) => void;
  readonly balancestrings_pending: (a: number, b: number) => void;
  readonly __wbg_balancestrings_free: (a: number, b: number) => void;
  readonly balance_mature: (a: number) => number;
  readonly balance_pending: (a: number) => number;
  readonly balance_outgoing: (a: number) => number;
  readonly balance_toBalanceStrings: (a: number, b: number, c: number) => void;
  readonly __wbg_balance_free: (a: number, b: number) => void;
  readonly __wbg_walletdescriptor_free: (a: number, b: number) => void;
  readonly __wbg_get_walletdescriptor_title: (a: number, b: number) => void;
  readonly __wbg_set_walletdescriptor_title: (a: number, b: number, c: number) => void;
  readonly __wbg_get_walletdescriptor_filename: (a: number, b: number) => void;
  readonly __wbg_set_walletdescriptor_filename: (a: number, b: number, c: number) => void;
  readonly __wbg_transactionrecordnotification_free: (a: number, b: number) => void;
  readonly __wbg_get_transactionrecordnotification_type: (a: number, b: number) => void;
  readonly __wbg_set_transactionrecordnotification_type: (a: number, b: number, c: number) => void;
  readonly __wbg_get_transactionrecordnotification_data: (a: number) => number;
  readonly __wbg_set_transactionrecordnotification_data: (a: number, b: number) => void;
  readonly __wbg_transactionrecord_free: (a: number, b: number) => void;
  readonly __wbg_get_transactionrecord_id: (a: number) => number;
  readonly __wbg_set_transactionrecord_id: (a: number, b: number) => void;
  readonly __wbg_get_transactionrecord_unixtimeMsec: (a: number, b: number) => void;
  readonly __wbg_set_transactionrecord_unixtimeMsec: (a: number, b: number, c: number) => void;
  readonly __wbg_get_transactionrecord_network: (a: number) => number;
  readonly __wbg_set_transactionrecord_network: (a: number, b: number) => void;
  readonly __wbg_get_transactionrecord_note: (a: number, b: number) => void;
  readonly __wbg_set_transactionrecord_note: (a: number, b: number, c: number) => void;
  readonly __wbg_get_transactionrecord_metadata: (a: number, b: number) => void;
  readonly __wbg_set_transactionrecord_metadata: (a: number, b: number, c: number) => void;
  readonly transactionrecord_value: (a: number) => number;
  readonly transactionrecord_blockDaaScore: (a: number) => number;
  readonly transactionrecord_binding: (a: number) => number;
  readonly transactionrecord_data: (a: number) => number;
  readonly transactionrecord_type: (a: number, b: number) => void;
  readonly transactionrecord_hasAddress: (a: number, b: number) => number;
  readonly transactionrecord_serialize: (a: number) => number;
  readonly argon2sha256ivFromText: (a: number, b: number, c: number, d: number) => void;
  readonly argon2sha256ivFromBinary: (a: number, b: number, c: number) => void;
  readonly sha256dFromText: (a: number, b: number, c: number) => void;
  readonly sha256dFromBinary: (a: number, b: number) => void;
  readonly sha256FromText: (a: number, b: number, c: number) => void;
  readonly sha256FromBinary: (a: number, b: number) => void;
  readonly decryptXChaCha20Poly1305: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly encryptXChaCha20Poly1305: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly __wbg_publickey_free: (a: number, b: number) => void;
  readonly publickey_try_new: (a: number, b: number, c: number) => void;
  readonly publickey_toString: (a: number, b: number) => void;
  readonly publickey_toAddress: (a: number, b: number, c: number) => void;
  readonly publickey_toAddressECDSA: (a: number, b: number, c: number) => void;
  readonly publickey_toXOnlyPublicKey: (a: number) => number;
  readonly publickey_fingerprint: (a: number) => number;
  readonly __wbg_xonlypublickey_free: (a: number, b: number) => void;
  readonly xonlypublickey_try_new: (a: number, b: number, c: number) => void;
  readonly xonlypublickey_toString: (a: number, b: number) => void;
  readonly xonlypublickey_toAddress: (a: number, b: number, c: number) => void;
  readonly xonlypublickey_toAddressECDSA: (a: number, b: number, c: number) => void;
  readonly xonlypublickey_fromAddress: (a: number, b: number) => void;
  readonly __wbg_privatekeygenerator_free: (a: number, b: number) => void;
  readonly privatekeygenerator_new: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly privatekeygenerator_receiveKey: (a: number, b: number, c: number) => void;
  readonly privatekeygenerator_changeKey: (a: number, b: number, c: number) => void;
  readonly __wbg_xprv_free: (a: number, b: number) => void;
  readonly xprv_try_new: (a: number, b: number) => void;
  readonly xprv_fromXPrv: (a: number, b: number, c: number) => void;
  readonly xprv_deriveChild: (a: number, b: number, c: number, d: number) => void;
  readonly xprv_derivePath: (a: number, b: number, c: number) => void;
  readonly xprv_intoString: (a: number, b: number, c: number, d: number) => void;
  readonly xprv_toString: (a: number, b: number) => void;
  readonly xprv_toXPub: (a: number, b: number) => void;
  readonly xprv_toPrivateKey: (a: number, b: number) => void;
  readonly xprv_privateKey: (a: number, b: number) => void;
  readonly xprv_depth: (a: number) => number;
  readonly xprv_parentFingerprint: (a: number, b: number) => void;
  readonly xprv_childNumber: (a: number) => number;
  readonly xprv_chainCode: (a: number, b: number) => void;
  readonly xprv_xprv: (a: number, b: number) => void;
  readonly __wbg_keypair_free: (a: number, b: number) => void;
  readonly keypair_get_public_key: (a: number, b: number) => void;
  readonly keypair_get_private_key: (a: number, b: number) => void;
  readonly keypair_get_xonly_public_key: (a: number) => number;
  readonly keypair_toAddress: (a: number, b: number, c: number) => void;
  readonly keypair_toAddressECDSA: (a: number, b: number, c: number) => void;
  readonly keypair_random: (a: number) => void;
  readonly keypair_fromPrivateKey: (a: number, b: number) => void;
  readonly __wbg_privatekey_free: (a: number, b: number) => void;
  readonly privatekey_try_new: (a: number, b: number, c: number) => void;
  readonly privatekey_toString: (a: number, b: number) => void;
  readonly privatekey_toKeypair: (a: number, b: number) => void;
  readonly privatekey_toPublicKey: (a: number, b: number) => void;
  readonly privatekey_toAddress: (a: number, b: number, c: number) => void;
  readonly privatekey_toAddressECDSA: (a: number, b: number, c: number) => void;
  readonly __wbg_derivationpath_free: (a: number, b: number) => void;
  readonly derivationpath_new: (a: number, b: number, c: number) => void;
  readonly derivationpath_isEmpty: (a: number) => number;
  readonly derivationpath_length: (a: number) => number;
  readonly derivationpath_parent: (a: number) => number;
  readonly derivationpath_push: (a: number, b: number, c: number, d: number) => void;
  readonly derivationpath_toString: (a: number, b: number) => void;
  readonly __wbg_xpub_free: (a: number, b: number) => void;
  readonly xpub_try_new: (a: number, b: number, c: number) => void;
  readonly xpub_deriveChild: (a: number, b: number, c: number, d: number) => void;
  readonly xpub_derivePath: (a: number, b: number, c: number) => void;
  readonly xpub_intoString: (a: number, b: number, c: number, d: number) => void;
  readonly xpub_toPublicKey: (a: number) => number;
  readonly xpub_xpub: (a: number, b: number) => void;
  readonly xpub_depth: (a: number) => number;
  readonly xpub_parentFingerprint: (a: number, b: number) => void;
  readonly xpub_childNumber: (a: number) => number;
  readonly xpub_chainCode: (a: number, b: number) => void;
  readonly __wbg_publickeygenerator_free: (a: number, b: number) => void;
  readonly publickeygenerator_fromXPub: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_fromMasterXPrv: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly publickeygenerator_receivePubkeys: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_receivePubkey: (a: number, b: number, c: number) => void;
  readonly publickeygenerator_receivePubkeysAsStrings: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_receivePubkeyAsString: (a: number, b: number, c: number) => void;
  readonly publickeygenerator_receiveAddresses: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly publickeygenerator_receiveAddress: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_receiveAddressAsStrings: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly publickeygenerator_receiveAddressAsString: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_changePubkeys: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_changePubkey: (a: number, b: number, c: number) => void;
  readonly publickeygenerator_changePubkeysAsStrings: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_changePubkeyAsString: (a: number, b: number, c: number) => void;
  readonly publickeygenerator_changeAddresses: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly publickeygenerator_changeAddress: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_changeAddressAsStrings: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly publickeygenerator_changeAddressAsString: (a: number, b: number, c: number, d: number) => void;
  readonly publickeygenerator_toString: (a: number, b: number) => void;
  readonly __wbg_pskt_free: (a: number, b: number) => void;
  readonly pskt_new: (a: number, b: number) => void;
  readonly pskt_role: (a: number, b: number) => void;
  readonly pskt_payload: (a: number) => number;
  readonly pskt_creator: (a: number, b: number) => void;
  readonly pskt_toConstructor: (a: number, b: number) => void;
  readonly pskt_toUpdater: (a: number, b: number) => void;
  readonly pskt_toSigner: (a: number, b: number) => void;
  readonly pskt_toCombiner: (a: number, b: number) => void;
  readonly pskt_toFinalizer: (a: number, b: number) => void;
  readonly pskt_toExtractor: (a: number, b: number) => void;
  readonly pskt_fallbackLockTime: (a: number, b: number, c: number) => void;
  readonly pskt_inputsModifiable: (a: number, b: number) => void;
  readonly pskt_outputsModifiable: (a: number, b: number) => void;
  readonly pskt_noMoreInputs: (a: number, b: number) => void;
  readonly pskt_noMoreOutputs: (a: number, b: number) => void;
  readonly pskt_input: (a: number, b: number, c: number) => void;
  readonly pskt_output: (a: number, b: number, c: number) => void;
  readonly pskt_setSequence: (a: number, b: number, c: number, d: number) => void;
  readonly pskt_calculateId: (a: number, b: number) => void;
  readonly version: (a: number) => void;
  readonly __wbg_nodedescriptor_free: (a: number, b: number) => void;
  readonly __wbg_get_nodedescriptor_uid: (a: number, b: number) => void;
  readonly __wbg_set_nodedescriptor_uid: (a: number, b: number, c: number) => void;
  readonly __wbg_get_nodedescriptor_url: (a: number, b: number) => void;
  readonly __wbg_set_nodedescriptor_url: (a: number, b: number, c: number) => void;
  readonly resolver_urls: (a: number) => number;
  readonly resolver_getNode: (a: number, b: number, c: number) => number;
  readonly resolver_getUrl: (a: number, b: number, c: number) => number;
  readonly resolver_connect: (a: number, b: number) => number;
  readonly resolver_ctor: (a: number, b: number) => void;
  readonly __wbg_resolver_free: (a: number, b: number) => void;
  readonly rpcclient_getBlockCount: (a: number, b: number) => number;
  readonly rpcclient_getBlockDagInfo: (a: number, b: number) => number;
  readonly rpcclient_getCoinSupply: (a: number, b: number) => number;
  readonly rpcclient_getConnectedPeerInfo: (a: number, b: number) => number;
  readonly rpcclient_getInfo: (a: number, b: number) => number;
  readonly rpcclient_getPeerAddresses: (a: number, b: number) => number;
  readonly rpcclient_getMetrics: (a: number, b: number) => number;
  readonly rpcclient_getConnections: (a: number, b: number) => number;
  readonly rpcclient_getSink: (a: number, b: number) => number;
  readonly rpcclient_getSinkBlueScore: (a: number, b: number) => number;
  readonly rpcclient_ping: (a: number, b: number) => number;
  readonly rpcclient_shutdown: (a: number, b: number) => number;
  readonly rpcclient_getServerInfo: (a: number, b: number) => number;
  readonly rpcclient_getSyncStatus: (a: number, b: number) => number;
  readonly rpcclient_addPeer: (a: number, b: number) => number;
  readonly rpcclient_ban: (a: number, b: number) => number;
  readonly rpcclient_estimateNetworkHashesPerSecond: (a: number, b: number) => number;
  readonly rpcclient_getBalanceByAddress: (a: number, b: number) => number;
  readonly rpcclient_getBalancesByAddresses: (a: number, b: number) => number;
  readonly rpcclient_getBlock: (a: number, b: number) => number;
  readonly rpcclient_getBlocks: (a: number, b: number) => number;
  readonly rpcclient_getBlockTemplate: (a: number, b: number) => number;
  readonly rpcclient_getCurrentBlockColor: (a: number, b: number) => number;
  readonly rpcclient_getDaaScoreTimestampEstimate: (a: number, b: number) => number;
  readonly rpcclient_getFeeEstimate: (a: number, b: number) => number;
  readonly rpcclient_getFeeEstimateExperimental: (a: number, b: number) => number;
  readonly rpcclient_getCurrentNetwork: (a: number, b: number) => number;
  readonly rpcclient_getHeaders: (a: number, b: number) => number;
  readonly rpcclient_getMempoolEntries: (a: number, b: number) => number;
  readonly rpcclient_getMempoolEntriesByAddresses: (a: number, b: number) => number;
  readonly rpcclient_getMempoolEntry: (a: number, b: number) => number;
  readonly rpcclient_getSubnetwork: (a: number, b: number) => number;
  readonly rpcclient_getUtxosByAddresses: (a: number, b: number) => number;
  readonly rpcclient_getVirtualChainFromBlock: (a: number, b: number) => number;
  readonly rpcclient_resolveFinalityConflict: (a: number, b: number) => number;
  readonly rpcclient_submitBlock: (a: number, b: number) => number;
  readonly rpcclient_submitTransaction: (a: number, b: number) => number;
  readonly rpcclient_submitTransactionReplacement: (a: number, b: number) => number;
  readonly rpcclient_unban: (a: number, b: number) => number;
  readonly rpcclient_subscribeBlockAdded: (a: number) => number;
  readonly rpcclient_unsubscribeBlockAdded: (a: number) => number;
  readonly rpcclient_subscribeFinalityConflict: (a: number) => number;
  readonly rpcclient_unsubscribeFinalityConflict: (a: number) => number;
  readonly rpcclient_subscribeFinalityConflictResolved: (a: number) => number;
  readonly rpcclient_unsubscribeFinalityConflictResolved: (a: number) => number;
  readonly rpcclient_subscribeSinkBlueScoreChanged: (a: number) => number;
  readonly rpcclient_unsubscribeSinkBlueScoreChanged: (a: number) => number;
  readonly rpcclient_subscribePruningPointUtxoSetOverride: (a: number) => number;
  readonly rpcclient_unsubscribePruningPointUtxoSetOverride: (a: number) => number;
  readonly rpcclient_subscribeNewBlockTemplate: (a: number) => number;
  readonly rpcclient_unsubscribeNewBlockTemplate: (a: number) => number;
  readonly rpcclient_subscribeVirtualDaaScoreChanged: (a: number) => number;
  readonly rpcclient_unsubscribeVirtualDaaScoreChanged: (a: number) => number;
  readonly rpcclient_subscribeUtxosChanged: (a: number, b: number) => number;
  readonly rpcclient_unsubscribeUtxosChanged: (a: number, b: number) => number;
  readonly rpcclient_subscribeVirtualChainChanged: (a: number, b: number) => number;
  readonly rpcclient_unsubscribeVirtualChainChanged: (a: number, b: number) => number;
  readonly rpcclient_defaultPort: (a: number, b: number, c: number) => void;
  readonly rpcclient_parseUrl: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly rpcclient_ctor: (a: number, b: number) => void;
  readonly rpcclient_url: (a: number, b: number) => void;
  readonly rpcclient_resolver: (a: number) => number;
  readonly rpcclient_setResolver: (a: number, b: number, c: number) => void;
  readonly rpcclient_setNetworkId: (a: number, b: number, c: number) => void;
  readonly rpcclient_isConnected: (a: number) => number;
  readonly rpcclient_encoding: (a: number, b: number) => void;
  readonly rpcclient_nodeId: (a: number, b: number) => void;
  readonly rpcclient_connect: (a: number, b: number) => number;
  readonly rpcclient_disconnect: (a: number) => number;
  readonly rpcclient_start: (a: number) => number;
  readonly rpcclient_stop: (a: number) => number;
  readonly rpcclient_triggerAbort: (a: number) => void;
  readonly rpcclient_addEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly rpcclient_removeEventListener: (a: number, b: number, c: number, d: number) => void;
  readonly rpcclient_clearEventListener: (a: number, b: number, c: number) => void;
  readonly rpcclient_removeAllEventListeners: (a: number, b: number) => void;
  readonly __wbg_rpcclient_free: (a: number, b: number) => void;
  readonly __wbg_createwritestreamoptions_free: (a: number, b: number) => void;
  readonly createwritestreamoptions_new_with_values: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => number;
  readonly createwritestreamoptions_auto_close: (a: number) => number;
  readonly createwritestreamoptions_set_auto_close: (a: number, b: number) => void;
  readonly createwritestreamoptions_emit_close: (a: number) => number;
  readonly createwritestreamoptions_set_emit_close: (a: number, b: number) => void;
  readonly createwritestreamoptions_encoding: (a: number) => number;
  readonly createwritestreamoptions_set_encoding: (a: number, b: number) => void;
  readonly createwritestreamoptions_fd: (a: number, b: number) => void;
  readonly createwritestreamoptions_set_fd: (a: number, b: number, c: number) => void;
  readonly createwritestreamoptions_flags: (a: number) => number;
  readonly createwritestreamoptions_set_flags: (a: number, b: number) => void;
  readonly createwritestreamoptions_mode: (a: number, b: number) => void;
  readonly createwritestreamoptions_set_mode: (a: number, b: number, c: number) => void;
  readonly createwritestreamoptions_start: (a: number, b: number) => void;
  readonly createwritestreamoptions_set_start: (a: number, b: number, c: number) => void;
  readonly __wbg_netserveroptions_free: (a: number, b: number) => void;
  readonly netserveroptions_allow_half_open: (a: number) => number;
  readonly netserveroptions_set_allow_half_open: (a: number, b: number) => void;
  readonly netserveroptions_pause_on_connect: (a: number) => number;
  readonly __wbg_userinfooptions_free: (a: number, b: number) => void;
  readonly userinfooptions_new_with_values: (a: number) => number;
  readonly userinfooptions_new: () => number;
  readonly userinfooptions_encoding: (a: number) => number;
  readonly userinfooptions_set_encoding: (a: number, b: number) => void;
  readonly __wbg_writefilesyncoptions_free: (a: number, b: number) => void;
  readonly writefilesyncoptions_new: (a: number, b: number, c: number, d: number) => number;
  readonly writefilesyncoptions_encoding: (a: number) => number;
  readonly writefilesyncoptions_set_encoding: (a: number, b: number) => void;
  readonly writefilesyncoptions_flag: (a: number) => number;
  readonly writefilesyncoptions_set_flag: (a: number, b: number) => void;
  readonly writefilesyncoptions_mode: (a: number, b: number) => void;
  readonly writefilesyncoptions_set_mode: (a: number, b: number, c: number) => void;
  readonly netserveroptions_set_pause_on_connect: (a: number, b: number) => void;
  readonly __wbg_setaadoptions_free: (a: number, b: number) => void;
  readonly setaadoptions_new: (a: number, b: number, c: number) => number;
  readonly setaadoptions_flush: (a: number) => number;
  readonly setaadoptions_set_flush: (a: number, b: number) => void;
  readonly setaadoptions_plaintextLength: (a: number) => number;
  readonly setaadoptions_set_plaintext_length: (a: number, b: number) => void;
  readonly setaadoptions_transform: (a: number) => number;
  readonly setaadoptions_set_transform: (a: number, b: number) => void;
  readonly __wbg_assertionerroroptions_free: (a: number, b: number) => void;
  readonly assertionerroroptions_new: (a: number, b: number, c: number, d: number) => number;
  readonly assertionerroroptions_message: (a: number) => number;
  readonly assertionerroroptions_set_message: (a: number, b: number) => void;
  readonly assertionerroroptions_actual: (a: number) => number;
  readonly assertionerroroptions_set_actual: (a: number, b: number) => void;
  readonly assertionerroroptions_expected: (a: number) => number;
  readonly assertionerroroptions_set_expected: (a: number, b: number) => void;
  readonly assertionerroroptions_operator: (a: number) => number;
  readonly assertionerroroptions_set_operator: (a: number, b: number) => void;
  readonly __wbg_createreadstreamoptions_free: (a: number, b: number) => void;
  readonly createreadstreamoptions_new_with_values: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number) => number;
  readonly createreadstreamoptions_auto_close: (a: number) => number;
  readonly createreadstreamoptions_set_auto_close: (a: number, b: number) => void;
  readonly createreadstreamoptions_emit_close: (a: number) => number;
  readonly createreadstreamoptions_set_emit_close: (a: number, b: number) => void;
  readonly createreadstreamoptions_encoding: (a: number) => number;
  readonly createreadstreamoptions_set_encoding: (a: number, b: number) => void;
  readonly createreadstreamoptions_end: (a: number, b: number) => void;
  readonly createreadstreamoptions_set_end: (a: number, b: number, c: number) => void;
  readonly createreadstreamoptions_fd: (a: number, b: number) => void;
  readonly createreadstreamoptions_set_fd: (a: number, b: number, c: number) => void;
  readonly createreadstreamoptions_flags: (a: number) => number;
  readonly createreadstreamoptions_set_flags: (a: number, b: number) => void;
  readonly createreadstreamoptions_high_water_mark: (a: number, b: number) => void;
  readonly createreadstreamoptions_set_high_water_mark: (a: number, b: number, c: number) => void;
  readonly createreadstreamoptions_mode: (a: number, b: number) => void;
  readonly createreadstreamoptions_set_mode: (a: number, b: number, c: number) => void;
  readonly createreadstreamoptions_start: (a: number, b: number) => void;
  readonly createreadstreamoptions_set_start: (a: number, b: number, c: number) => void;
  readonly __wbg_processsendoptions_free: (a: number, b: number) => void;
  readonly processsendoptions_new: (a: number) => number;
  readonly processsendoptions_swallow_errors: (a: number) => number;
  readonly processsendoptions_set_swallow_errors: (a: number, b: number) => void;
  readonly __wbg_wasioptions_free: (a: number, b: number) => void;
  readonly wasioptions_new_with_values: (a: number, b: number, c: number, d: number) => number;
  readonly wasioptions_new: (a: number) => number;
  readonly wasioptions_args: (a: number, b: number) => void;
  readonly wasioptions_set_args: (a: number, b: number, c: number) => void;
  readonly wasioptions_env: (a: number) => number;
  readonly wasioptions_set_env: (a: number, b: number) => void;
  readonly wasioptions_preopens: (a: number) => number;
  readonly wasioptions_set_preopens: (a: number, b: number) => void;
  readonly __wbg_appendfileoptions_free: (a: number, b: number) => void;
  readonly appendfileoptions_new_with_values: (a: number, b: number, c: number, d: number) => number;
  readonly appendfileoptions_new: () => number;
  readonly appendfileoptions_encoding: (a: number) => number;
  readonly appendfileoptions_set_encoding: (a: number, b: number) => void;
  readonly appendfileoptions_mode: (a: number, b: number) => void;
  readonly appendfileoptions_set_mode: (a: number, b: number, c: number) => void;
  readonly appendfileoptions_flag: (a: number) => number;
  readonly appendfileoptions_set_flag: (a: number, b: number) => void;
  readonly __wbg_formatinputpathobject_free: (a: number, b: number) => void;
  readonly formatinputpathobject_new_with_values: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly formatinputpathobject_new: () => number;
  readonly formatinputpathobject_base: (a: number) => number;
  readonly formatinputpathobject_set_base: (a: number, b: number) => void;
  readonly formatinputpathobject_dir: (a: number) => number;
  readonly formatinputpathobject_set_dir: (a: number, b: number) => void;
  readonly formatinputpathobject_ext: (a: number) => number;
  readonly formatinputpathobject_set_ext: (a: number, b: number) => void;
  readonly formatinputpathobject_name: (a: number) => number;
  readonly formatinputpathobject_set_name: (a: number, b: number) => void;
  readonly formatinputpathobject_root: (a: number) => number;
  readonly formatinputpathobject_set_root: (a: number, b: number) => void;
  readonly __wbg_getnameoptions_free: (a: number, b: number) => void;
  readonly getnameoptions_new: (a: number, b: number, c: number, d: number) => number;
  readonly getnameoptions_family: (a: number) => number;
  readonly getnameoptions_set_family: (a: number, b: number) => void;
  readonly getnameoptions_host: (a: number) => number;
  readonly getnameoptions_set_host: (a: number, b: number) => void;
  readonly getnameoptions_local_address: (a: number) => number;
  readonly getnameoptions_set_local_address: (a: number, b: number) => void;
  readonly getnameoptions_port: (a: number) => number;
  readonly getnameoptions_set_port: (a: number, b: number) => void;
  readonly __wbg_mkdtempsyncoptions_free: (a: number, b: number) => void;
  readonly mkdtempsyncoptions_new_with_values: (a: number) => number;
  readonly mkdtempsyncoptions_new: () => number;
  readonly mkdtempsyncoptions_encoding: (a: number) => number;
  readonly mkdtempsyncoptions_set_encoding: (a: number, b: number) => void;
  readonly writestream_add_listener_with_open: (a: number, b: number) => number;
  readonly writestream_add_listener_with_close: (a: number, b: number) => number;
  readonly writestream_on_with_open: (a: number, b: number) => number;
  readonly writestream_on_with_close: (a: number, b: number) => number;
  readonly writestream_once_with_open: (a: number, b: number) => number;
  readonly writestream_once_with_close: (a: number, b: number) => number;
  readonly writestream_prepend_listener_with_open: (a: number, b: number) => number;
  readonly writestream_prepend_listener_with_close: (a: number, b: number) => number;
  readonly writestream_prepend_once_listener_with_open: (a: number, b: number) => number;
  readonly writestream_prepend_once_listener_with_close: (a: number, b: number) => number;
  readonly __wbg_consoleconstructoroptions_free: (a: number, b: number) => void;
  readonly consoleconstructoroptions_new_with_values: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly consoleconstructoroptions_new: (a: number, b: number) => number;
  readonly consoleconstructoroptions_stdout: (a: number) => number;
  readonly consoleconstructoroptions_set_stdout: (a: number, b: number) => void;
  readonly consoleconstructoroptions_stderr: (a: number) => number;
  readonly consoleconstructoroptions_set_stderr: (a: number, b: number) => void;
  readonly consoleconstructoroptions_ignore_errors: (a: number) => number;
  readonly consoleconstructoroptions_set_ignore_errors: (a: number, b: number) => void;
  readonly consoleconstructoroptions_color_mod: (a: number) => number;
  readonly consoleconstructoroptions_set_color_mod: (a: number, b: number) => void;
  readonly consoleconstructoroptions_inspect_options: (a: number) => number;
  readonly consoleconstructoroptions_set_inspect_options: (a: number, b: number) => void;
  readonly __wbg_pipeoptions_free: (a: number, b: number) => void;
  readonly pipeoptions_new: (a: number) => number;
  readonly pipeoptions_end: (a: number) => number;
  readonly pipeoptions_set_end: (a: number, b: number) => void;
  readonly __wbg_agentconstructoroptions_free: (a: number, b: number) => void;
  readonly agentconstructoroptions_keep_alive_msecs: (a: number) => number;
  readonly agentconstructoroptions_set_keep_alive_msecs: (a: number, b: number) => void;
  readonly agentconstructoroptions_keep_alive: (a: number) => number;
  readonly agentconstructoroptions_set_keep_alive: (a: number, b: number) => void;
  readonly agentconstructoroptions_max_free_sockets: (a: number) => number;
  readonly agentconstructoroptions_set_max_free_sockets: (a: number, b: number) => void;
  readonly agentconstructoroptions_max_sockets: (a: number) => number;
  readonly agentconstructoroptions_set_max_sockets: (a: number, b: number) => void;
  readonly agentconstructoroptions_timeout: (a: number) => number;
  readonly agentconstructoroptions_set_timeout: (a: number, b: number) => void;
  readonly __wbg_createhookcallbacks_free: (a: number, b: number) => void;
  readonly createhookcallbacks_new: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly createhookcallbacks_init: (a: number) => number;
  readonly createhookcallbacks_set_init: (a: number, b: number) => void;
  readonly createhookcallbacks_before: (a: number) => number;
  readonly createhookcallbacks_set_before: (a: number, b: number) => void;
  readonly createhookcallbacks_after: (a: number) => number;
  readonly createhookcallbacks_set_after: (a: number, b: number) => void;
  readonly createhookcallbacks_destroy: (a: number) => number;
  readonly createhookcallbacks_set_destroy: (a: number, b: number) => void;
  readonly createhookcallbacks_promise_resolve: (a: number) => number;
  readonly createhookcallbacks_set_promise_resolve: (a: number, b: number) => void;
  readonly __wbg_streamtransformoptions_free: (a: number, b: number) => void;
  readonly streamtransformoptions_new: (a: number, b: number) => number;
  readonly streamtransformoptions_flush: (a: number) => number;
  readonly streamtransformoptions_set_flush: (a: number, b: number) => void;
  readonly streamtransformoptions_transform: (a: number) => number;
  readonly streamtransformoptions_set_transform: (a: number, b: number) => void;
  readonly readstream_add_listener_with_open: (a: number, b: number) => number;
  readonly readstream_add_listener_with_close: (a: number, b: number) => number;
  readonly readstream_on_with_open: (a: number, b: number) => number;
  readonly readstream_on_with_close: (a: number, b: number) => number;
  readonly readstream_once_with_open: (a: number, b: number) => number;
  readonly readstream_once_with_close: (a: number, b: number) => number;
  readonly readstream_prepend_listener_with_open: (a: number, b: number) => number;
  readonly readstream_prepend_listener_with_close: (a: number, b: number) => number;
  readonly readstream_prepend_once_listener_with_open: (a: number, b: number) => number;
  readonly readstream_prepend_once_listener_with_close: (a: number, b: number) => number;
  readonly rustsecp256k1_v0_10_0_context_create: (a: number) => number;
  readonly rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
  readonly rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
  readonly rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
  readonly __wbg_aborted_free: (a: number, b: number) => void;
  readonly __wbg_abortable_free: (a: number, b: number) => void;
  readonly abortable_new: () => number;
  readonly abortable_isAborted: (a: number) => number;
  readonly abortable_abort: (a: number) => void;
  readonly abortable_check: (a: number, b: number) => void;
  readonly abortable_reset: (a: number) => void;
  readonly setLogLevel: (a: number) => void;
  readonly defer: () => number;
  readonly presentPanicHookLogs: () => void;
  readonly initConsolePanicHook: () => void;
  readonly initBrowserPanicHook: () => void;
  readonly initWASM32Bindings: (a: number, b: number) => void;
  readonly __wbindgen_export_0: (a: number, b: number) => number;
  readonly __wbindgen_export_1: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_export_2: WebAssembly.Table;
  readonly __wbindgen_export_3: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_4: (a: number, b: number) => void;
  readonly __wbindgen_export_5: (a: number, b: number, c: number) => void;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
  readonly __wbindgen_export_6: (a: number, b: number, c: number, d: number) => void;
  readonly __wbindgen_export_7: (a: number, b: number) => void;
  readonly __wbindgen_export_8: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_9: (a: number, b: number) => void;
  readonly __wbindgen_export_10: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_11: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_export_12: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_13: (a: number, b: number) => void;
  readonly __wbindgen_export_14: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_15: (a: number, b: number) => void;
  readonly __wbindgen_export_16: (a: number) => void;
  readonly __wbindgen_export_17: (a: number, b: number, c: number) => void;
  readonly __wbindgen_export_18: (a: number, b: number, c: number, d: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
