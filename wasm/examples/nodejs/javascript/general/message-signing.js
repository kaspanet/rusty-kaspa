let kaspa = require('../../../../nodejs/kaspa');
let {
    PrivateKey,
    PublicKey,
    signMessage,
    verifyMessage,
} = kaspa;

kaspa.initConsolePanicHook();

let message = 'Hello Kaspa!';
let privkey = 'b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef';
let pubkey = 'dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659';

function runDemo(message, privateKey, publicKey, noAuxRand) {
    let signature = signMessage({message, privateKey, noAuxRand});

    console.info(`Message: ${message} => Signature: ${signature}`);

    if (verifyMessage({message, signature, publicKey})) {
        console.info('Signature verified!');
    } else {
        console.info('Signature is invalid!');
    }
}

// Using strings:
runDemo(message, privkey, pubkey);
runDemo(message, privkey, pubkey, true);
// Using Objects:
runDemo(message, new PrivateKey(privkey), new PublicKey(pubkey));
runDemo(message, new PrivateKey(privkey), new PublicKey(pubkey), true);
