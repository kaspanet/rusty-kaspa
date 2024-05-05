import {
    PrivateKey,
    PublicKey,
    PublicKeyGenerator,
    createAddress,
    NetworkType,
    initConsolePanicHook
} from "../../../../nodejs/kaspa";

initConsolePanicHook();

(async () => {
    /*** Common Use-cases ***/
    demoGenerateAddressFromPrivateKeyHexString();
    demoGenerateAddressFromPublicKeyHexString();

    /*** Advanced ***/
    // HD Wallet-style public key generation
    let xpub: PublicKeyGenerator = await PublicKeyGenerator.fromMasterXPrv(
        "kprv5y2qurMHCsXYrNfU3GCihuwG3vMqFji7PZXajMEqyBkNh9UZUJgoHYBLTKu1eM4MvUtomcXPQ3Sw9HZ5ebbM4byoUciHo1zrPJBQfqpLorQ",
        false,
        0n
    );

    console.log("xpub", xpub)

    // Generates the first 10 Receive Public keys and their addresses
    let compressedPublicKeys: string[] = await xpub.receivePubkeys(0, 10);
    let addresses: string[] = compressedPublicKeys.map(key => createAddress(key, NetworkType.Mainnet).toString());
    console.log("receive addresses", addresses);

    // Generates the first 10 Change Public keys and their addresses
    let compressedChangePublicKeys: string[] = await xpub.changePubkeys(0, 10);
    console.log("change address compressedChangePublicKeys", compressedChangePublicKeys)
    addresses = compressedChangePublicKeys.map(key => createAddress(key, NetworkType.Mainnet).toString());
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
    console.info(publicKey.toAddress(NetworkType.Mainnet).toString() == 'kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva');

    // Given x-only public key: 'dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'
    const xOnlyPublicKey = new PublicKey('dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659');
    console.info("Given x-only public key: 'dff1d77f2a671c5f36183726db2341be58feae1da2deced843240f7b502ba659'");
    console.info(xOnlyPublicKey.toString());
    console.info(xOnlyPublicKey.toAddress(NetworkType.Mainnet).toString());
    console.info(xOnlyPublicKey.toAddress(NetworkType.Mainnet).toString() == 'kaspa:qr0lr4ml9fn3chekrqmjdkergxl93l4wrk3dankcgvjq776s9wn9jkdskewva');

    // Given full DER public key: '0421eb0c4270128b16c93c5f0dac48d56051a6237dae997b58912695052818e348b0a895cbd0c93a11ee7afac745929d96a4642a71831f54a7377893af71a2e2ae'
    const fullDERPublicKey = new PublicKey('0421eb0c4270128b16c93c5f0dac48d56051a6237dae997b58912695052818e348b0a895cbd0c93a11ee7afac745929d96a4642a71831f54a7377893af71a2e2ae');
    console.info("Given x-only public key: '0421eb0c4270128b16c93c5f0dac48d56051a6237dae997b58912695052818e348b0a895cbd0c93a11ee7afac745929d96a4642a71831f54a7377893af71a2e2ae'");
    console.info(fullDERPublicKey.toString());
    console.info(fullDERPublicKey.toAddress(NetworkType.Mainnet).toString());
    console.info(fullDERPublicKey.toAddress(NetworkType.Mainnet).toString() == 'kaspa:qqs7krzzwqfgk9kf830smtzg64s9rf3r0khfj76cjynf2pfgrr35saatu88xq');
}
