let kaspa = require('./kaspa/kaspa_wasm');
let {
    PrivateKey,
    PublicKey,
    signMessage,
    verifyMessage,
} = kaspa;

kaspa.initConsolePanicHook();

let message = 'Hello Kaspa!';
let privkey = 'B7E151628AED2A6ABF7158809CF4F3C762E7160F38B4DA56A784D9045190CFEF';
let pubkey = 'DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659';

function runDemo(message, privateKey, publicKey) {
    let signature = signMessage({message, privateKey});

    console.info(`Message: ${message} => Signature: ${signature}`);

    if (verifyMessage({message, signature, publicKey})) {
        console.info('Signature verified!');
    } else {
        console.info('Signature is invalid!');
    }
}

// Using strings:
runDemo(message, privkey, pubkey);
// Using Objects:
runDemo(message, new PrivateKey(privkey), new PublicKey(pubkey));
