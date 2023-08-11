const path = require('path');

const {
    Address,
    Encoding,
    NetworkType,
} = require('./kaspa/kaspa_wasm');

function parseArgs(options = {}) {
    const script = path.basename(process.argv[1]);

    let args = process.argv.slice(2);
    if (args.includes('--help')) {
        console.log(`Usage: node ${script} [address] [mainnet|testnet] [--json] ${options.additionalHelpOutput}`);
        process.exit(0);
    }

    let addressRegex = new RegExp(/(kaspa|kaspatest):\S+/i);
    const addressArg = args.find((arg, i) => i === 0 && addressRegex.test(arg));
    const networkArg = args.find((arg, i) => (i === 0 || i === 1) && (arg === 'mainnet' || arg === 'testnet'));

    let address = null;
    if (addressArg !== undefined) {
        if (addressRegex.test(addressArg)) {
            address = new Address(addressArg);
        }
    }

    // by default, use testnet
    let networkType = NetworkType.Testnet;
    // if "mainnet" is specified or if address starts with "kaspa:" use mainnet
    if (networkArg !== undefined) {
        if (networkArg === 'mainnet' || (addressArg !== undefined && addressArg.startsWith('kaspa:'))) {
            networkType = NetworkType.Mainnet;
        }
    }

    let encoding = Encoding.Borsh;

    const jsonArgIdx = args.findIndex((arg) => arg === '--json');
    if (jsonArgIdx !== -1) {
        encoding = Encoding.SerdeJson;
    }

    return {
        address,
        networkType,
        encoding,
    };
}

module.exports = {
    parseArgs,
};
