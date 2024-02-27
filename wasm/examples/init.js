
const fs = require('fs');
const path = require('path');
const { Mnemonic, XPrv, PublicKeyGenerator } = require('../nodejs/kaspa');
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

const configFileName = path.join(__dirname, "data", "config.json");
const exists = fs.existsSync(configFileName);
if (!exists || values.reset) {
    createConfigFile();
    process.exit(0);
}

if (network) {
    let config = JSON.parse(fs.readFileSync(configFileName, "utf8"));
    config.networkId = network;
    fs.writeFileSync(configFileName, JSON.stringify(config, null, 4));
    console.log("");
    console.log(`Updating networkId to '${network}'`);
}

if (fs.existsSync(configFileName)) {
    let config = JSON.parse(fs.readFileSync(configFileName, "utf8"));
// console.log("loading mnemonic:", config.mnemonic);
    let mnemonic = new Mnemonic(config.mnemonic);
    let wallet = basicWallet(config.networkId, mnemonic);

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
    
    if (!network) {
        console.log("... '--network=' argument is not specified ...defaulting to 'testnet-11'");
    }
    let networkId = network ?? "testnet-11";

    // console.log("Creating Mnemonic:", Mnemonic.random());
    // console.log("Mnemonic:", Mnemonic);
    let wallet = basicWallet(networkId, Mnemonic.random());

    let config = {
        networkId,
        mnemonic: wallet.mnemonic.phrase,
        // xprv: wallet.xprv,
        // receive: wallet.receive,
        // change: wallet.change,
    };
    fs.writeFileSync(configFileName, JSON.stringify(config, null, 4));
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

function basicWallet(networkId, mnemonic) {
    console.log("mnemonic:", Mnemonic);
    console.log("mnemonic:", mnemonic.phrase);
    let xprv = new XPrv(mnemonic.toSeed());
    let account_0 = xprv.derivePath("m/44'/111111'/0'/0").toXPub();
    let receive_xpub = account_0.deriveChild(0);
    // console.log("receive_xpub:", receive_xpub.toString());
    let change_xpub = account_0.deriveChild(1);
    // console.log("change_xpub:", change_xpub.toString());

    let receive_pubkey = receive_xpub.deriveChild(0).publicKey();
    let change_pubkey = change_xpub.deriveChild(0).publicKey();
    // console.log("receive_pubkey:", receive_pubkey.toString());
    // console.log("change_pubkey:", change_pubkey.toString());

    let receive = receive_pubkey.toAddress(networkId).toString();
    let change = change_pubkey.toAddress(networkId).toString();
    // console.log("receive_address:", receive_address);
    // console.log("change_address:", change_address);
    // account_0.deriveChild(0).toAddress(networkId);
    // account_0.deriveChild(0).toAddress(networkId);
    // account_0.toXPub().publicKey().toAddress(networkId);
    // let xpub = account_0.publicKey();
    // let address = xpub.deriveChild(0).toAddress();

    let keygen = PublicKeyGenerator.fromMasterXPrv(
        xprv.toString(),
        false,
        0n,0
    );

    // let receive_pubkeys = keygen.receivePubkeys(0,1).map((key) => key.toString());
    // let change_pubkeys = keygen.changePubkeys(0,1).map((key) => key.toString());
    let receive_pubkeys = keygen.receivePubkeys(0,1).map((key) => key.toAddress(networkId).toString());
    let change_pubkeys = keygen.changePubkeys(0,1).map((key) => key.toAddress(networkId).toString());
    console.log("receive_pubkeys:", receive_pubkeys);
    console.log("change_pubkeys:", change_pubkeys);


    return {
        mnemonic,
        xprv: xprv.toString(),
        receive,
        change,
        // xprv : xprv.intoString("xprv"),
        // xpub,
        // address,  // receive address
    };
}