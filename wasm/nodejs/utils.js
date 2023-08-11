const path = require('path');

const {
    Address,
    Encoding,
    NetworkType,
} = require('./kaspa/kaspa_wasm');

function parseArgs() {

    const script = path.basename(process.argv[1]);

    let args = process.argv.slice(2).join(" ");

    if (args.match(/--help/i)) {
        console.log(`Usage: node ${script} [address] [mainnet|testnet] [--json]`);
    }

    let address = null;
    let addressRegex = new RegExp(/(kaspa|kaspatest):\S+/i);
    if (args.match(addressRegex)) {
        address = new Address(args.match(addressRegex)[0]);
    }

    let networkType = NetworkType = NetworkType.Mainnet;
    if (args.match(/mainnet/i)) {
        networkType = NetworkType.Mainnet;
    } else if (address && address.startsWith("kaspa:")) {
        networkType = NetworkType.Mainnet;
    }

    let encoding = Encoding.Borsh;
    if (args.match(/--json/i)) {
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
