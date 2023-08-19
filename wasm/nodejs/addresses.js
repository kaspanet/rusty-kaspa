globalThis.WebSocket = require('websocket').w3cwebsocket;

let kaspa = require('./kaspa/kaspa_wasm');
let {
    PrivateKey,
    PublicKey,
    XPublicKey,
    createAddress,
    NetworkType,
} = kaspa;
kaspa.init_console_panic_hook();

(async ()=>{
    /*** Common Use-cases ***/
    demoGenerateAddressFromPrivateKeyHexString();
    demoGenerateAddressFromPublicKeyHexString();

    /*** Advanced ***/
    // HD Wallet-style public key generation
    let xpub = await XPublicKey.fromMasterXPrv(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    console.log("xpub", xpub)

    // Generates the first 10 Receive Public keys and their addresses
    let compressedPublicKeys = await xpub.receivePubkeys(0, 10);
    console.log("receive address compressedPublicKeys", compressedPublicKeys);
    let addresses = compressedPublicKeys.map(key=>createAddress(key, NetworkType.Mainnet).toString());
    console.log("receive addresses", addresses);

    // Generates the first 10 Change Public keys and their addresses
    compressedPublicKeys = await xpub.changePubkeys(0, 10);
    console.log("change address compressedPublicKeys", compressedPublicKeys)
    addresses = compressedPublicKeys.map(key=>createAddress(key, NetworkType.Mainnet).toString());
    console.log("change addresses", addresses);

})();

// Getting Kaspa Address from Private Key
function demoGenerateAddressFromPrivateKeyHexString() {
    // From Hex string
    const privateKey = new PrivateKey('b7e151628aed2a6abf7158809cf4f3c762e7160f38b4da56a784d9045190cfef'); // From BIP0340
    console.info(privateKey.toKeypair().toAddress(NetworkType.Mainnet).toString());
}

function demoGenerateAddressFromPublicKeyHexString() {
    // Given compressed public key: '02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'
    const publicKey = new PublicKey('02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659');
    console.info("Given compressed public key: '02dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'");
    console.info(publicKey.toString());
    console.info(publicKey.toAddress(NetworkType.Mainnet).toString());

    // Given x-only public key: 'dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'
    const xOnlyPublicKey = new PublicKey('dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659');
    console.info("Given x-only public key: 'dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'");
    console.info(xOnlyPublicKey.toString());
    console.info(xOnlyPublicKey.toAddress(NetworkType.Mainnet).toString());
}