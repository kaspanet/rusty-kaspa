const path = require('path');
const nodeUtil = require('node:util');
const {parseArgs: nodeParseArgs} = nodeUtil;

const {
    Address,
    Encoding,
    NetworkType,
} = require('./kaspa/kaspa_wasm');

/**
 * Helper function to parse command line arguments for running the scripts
 * @param options Additional options to configure the parsing, such as additional arguments for the script and additional help output to go with it
 * @returns {{address: Address, tokens: NodeUtilParseArgsToken[], networkType: (NetworkType), encoding: (Encoding)}}
 */
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
            destination: {
                type: 'string',
            },
            network: {
                type: 'string',
            },
        }, tokens: true, allowPositionals: true
    });
    if (values.help) {
        console.log(`Usage: node ${script} [address] [mainnet|testnet] [--destination <address>] [--network <mainnet|testnet>] [--json] ${options.additionalHelpOutput}`);
        process.exit(0);
    }

    const addressRegex = new RegExp(/(kaspa|kaspatest):\S+/i);
    const addressArg = values.address ?? positionals.find((positional) => addressRegex.test(positional)) ?? null;
    const destinationAddress = addressArg === null ? null : new Address(addressArg);

    let networkType = addressArg?.startsWith('kaspa:') ? NetworkType.Mainnet : NetworkType.Testnet;
    const networkArg = values.network ?? positionals.find((positional) => positional === 'mainnet' || positional === 'testnet') ?? null;
    if (networkArg !== null) {
        networkType = networkArg === 'mainnet' ? NetworkType.Mainnet : NetworkType.Testnet;
    }

    const encoding = values.json ? Encoding.SerdeJson : Encoding.Borsh;

    return {
        destinationAddress,
        networkType,
        encoding,
        tokens,
    };
}

module.exports = {
    parseArgs,
};
