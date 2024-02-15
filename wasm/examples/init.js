
const fs = require('fs');
const { Mnemonic, XPrv } = require('../nodejs/kaspa');
const { parseArgs } = require('node:util');
const { create } = require('domain');

let args = process.argv.slice(2);
const {
    values,
    positionals,
    tokens,
} = parseArgs({
    args, options: {
        help: {
            type: 'boolean',
        },
        reset: {
            type: 'boolean',
        },
        network: {
            type: 'string',
        },
    }, tokens: true, allowPositionals: true
});

if (values.help) {
    console.log(`Usage: node init [--reset] [--network=(mainnet|testnet-<number>)]`);
    process.exit(0);
}

const network = values.network ?? positionals.find((positional) => positional.match(/^(testnet|mainnet|simnet|devnet)-\d+$/)) ?? null;

const exists = fs.existsSync("./data/config.json");
if (!exists || values.reset) {
    createConfigFile();
    process.exit(0);
}

if (network) {
    let config = JSON.parse(fs.readFileSync("./data/config.json", "utf8"));
    config.networkId = network;
    fs.writeFileSync("./data/config.json", JSON.stringify(config, null, 4));
    console.log("");
    console.log(`Updating networkId to '${network}'`);
}

if (fs.existsSync("./data/config.json")) {
    let config = JSON.parse(fs.readFileSync("./data/config.json", "utf8"));

    let mnemonic = new Mnemonic(config.mnemonic);
    let wallet = basicWallet(mnemonic);

    console.log("");
    console.log("networkId:", config.networkId);
    console.log("mnemonic:", wallet.mnemonic.phrase);
    console.log("xprv:", wallet.xprv);
    console.log("receive:", wallet.receive);
    console.log("change:", wallet.change);
    console.log("");
    console.log("Use 'init --reset' to reset the config file");
    console.log("");
}

function createConfigFile() {
    let wallet = basicWallet(Mnemonic.random());

    if (!network) {
        console.log("... '--network=' argument is not specified ...defaulting to 'testnet-11'");
    }

    let networkId = network ?? "testnet-11";
    let config = {
        networkId,
        mnemonic: wallet.mnemonic.phrase,
        xprv: wallet.xprv,
        receive: wallet.receive,
        change: wallet.change,
    };
    fs.writeFileSync("./data/config.json", JSON.stringify(config, null, 4));
    console.log("");
    console.log("Creating config data in './data/config.json'");
    console.log("");
    console.log("networkId:", networkId);
    console.log("mnemonic:", wallet.mnemonic.phrase);
    console.log("xprv:", wallet.xprv);
    console.log("receive:", wallet.receive);
    console.log("change:", wallet.change);
    console.log("");
}

function basicWallet(mnemonic) {
    let xprv = new XPrv(mnemonic.toSeed());
    let account_0 = xprv.derivePath("m/44'/111111'/0'/0");
    // let xpub = account_0.publicKey();
    // let address = xpub.deriveChild(0).toAddress();

    return {
        mnemonic,
        xprv: xprv.toString(),
        // xprv : xprv.intoString("xprv"),
        // xpub,
        // address,  // receive address
    };
}