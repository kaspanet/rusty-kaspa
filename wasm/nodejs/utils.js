const path = require('path');
const nodeUtil = require('node:util');
const {parseArgs: nodeParseArgs} = nodeUtil;

const {
    Address,
    Encoding,
    NetworkType,
} = require('./kaspa/kaspa_wasm');

function parseArgs(options = {
    additionalParseArgs: {},
    additionalHelpOutput: '',
}) {
    const script = path.basename(process.argv[1]);
    let args = process.argv.slice(2);
    const {
        values,
        positionals,
        tokens,
    } = nodeParseArgs({
        args, options: {
            ...options.additionalParseArgs,
            help: {
                type: 'boolean',
            },
            json: {
                type: 'boolean',
            },
            address: {
                type: 'string',
            },
            network: {
                type: 'string',
            },
        }, tokens: true, allowPositionals: true
    });
    if (values.help) {
        console.log(`Usage: node ${script} [address] [mainnet|testnet] [--address ADDRESS] [--network mainnet|testnet] [--json] ${options.additionalHelpOutput}`);
        process.exit(0);
    }

    const addressRegex = new RegExp(/(kaspa|kaspatest):\S+/i);
    const addressArg = values.address ?? positionals.find((positional) => addressRegex.test(positional)) ?? null;
    const address = addressArg === null ? null : new Address(addressArg);

    let networkType = addressArg?.startsWith('kaspa:') ? NetworkType.Mainnet : NetworkType.Testnet;
    const networkArg = values.network ?? positionals.find((positional) => positional === 'mainnet' || positional === 'testnet') ?? null;
    if (networkArg !== null) {
        networkType = networkArg === 'mainnet' ? NetworkType.Mainnet : NetworkType.Testnet;
    }

    const encoding = values.json ? Encoding.SerdeJson : Encoding.Borsh;

    return {
        address,
        networkType,
        encoding,
        tokens,
    };
}

module.exports = {
    parseArgs,
};
