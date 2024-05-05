import * as path from 'path';
import * as nodeUtil from 'node:util';
const { parseArgs: nodeParseArgs } = nodeUtil;

import {
    Address,
    Encoding,
    NetworkId,
} from "../../../../nodejs/kaspa";

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
        args,
        options: {
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
            encoding: {
                type: 'string',
            },
            address:{
                type: 'string'
            }
        },
        tokens: true,
        allowPositionals: true
    });
    if (values.help) {
        console.log(`Usage: node ${script} [address] [mainnet|testnet] [--destination <address>] [--network <mainnet|testnet>] [--encoding <borsh|json>] ${options.additionalHelpOutput}`);
        process.exit(0);
    }

    const addressRegex = new RegExp(/(kaspa|kaspatest):\S+/i);
    const addressArg = values.address ?? positionals.find((positional) => addressRegex.test(positional)) ?? null;
    const destinationAddress = addressArg === null ? null : new Address(addressArg);

    const networkArg = values.network ?? positionals.find((positional) => positional.match(/^(testnet|mainnet|simnet|devnet)-\d+$/)) ?? null;
    if (!networkArg) {
        console.error('Network id must be specified: --network=(mainnet|testnet-<number>)');
        process.exit(1);
    }
    const networkId = new NetworkId(networkArg);

    const encodingArg = values.encoding ?? positionals.find((positional) => positional.match(/^(borsh|json)$/)) ?? null;
    let encoding = Encoding.Borsh;
    if (encodingArg == "json") {
        encoding = Encoding.SerdeJson;
    }

    return {
        destinationAddress,
        networkId,
        encoding,
        tokens,
    };
}

export {
    parseArgs,
}
