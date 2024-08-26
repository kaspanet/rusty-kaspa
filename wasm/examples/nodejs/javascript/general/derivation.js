const kaspa = require('../../../../nodejs/kaspa');
const {
    Mnemonic,
    XPrv,
    DerivationPath,
    PublicKey,
    NetworkType,
} = kaspa;

kaspa.initConsolePanicHook();

(async () => {

    //const mnemonic = Mnemonic.random();
    const mnemonic = new Mnemonic("hunt bitter praise lift buyer topic crane leopard uniform network inquiry over grain pass match crush marine strike doll relax fortune trumpet sunny silk")
    console.log("mnemonic:", mnemonic);
    const seed = mnemonic.toSeed();
    console.log("seed:", seed);

    // kaspa
    let xPrv = new XPrv(seed);
    // derive full path upto second address of receive wallet
    let pubkey1 = xPrv.derivePath("m/44'/111111'/0'/0/1").toXPub().toPublicKey();
    console.log("publickey", pubkey1.toString())
    console.log("address", pubkey1.toAddress(NetworkType.Mainnet));

    // create receive wallet
    let receiveWalletXPub = xPrv.derivePath("m/44'/111111'/0'/0").toXPub();
    // derive receive wallet for second address
    let pubkey2 = receiveWalletXPub.deriveChild(1, false).toPublicKey();
    console.log("address", pubkey2.toAddress(NetworkType.Mainnet));
    if (pubkey1.toString() != pubkey2.toString()){
        throw new Error("pubkey2 dont match")
    }

    // create change wallet
    let changeWalletXPub = xPrv.derivePath("m/44'/111111'/0'/1").toXPub();
    // derive change wallet for first address
    let pubkey3 = changeWalletXPub.deriveChild(0, false).toPublicKey();
    console.log("change address", pubkey3.toAddress(NetworkType.Mainnet));
    // ---

    //drive address via private key
    let privateKey = xPrv.derivePath("m/44'/111111'/0'/0/1").toPrivateKey();
    console.log("address via private key", privateKey.toAddress(NetworkType.Mainnet))
    console.log("privatekey", privateKey.toString());
    let pubkey4 = privateKey.toPublicKey();
    if (pubkey1.toString() != pubkey4.toString()){
        throw new Error("pubkey4 dont match")
    }

    // xprv with ktrv prefix
    const ktrv = xPrv.intoString("ktrv");
    console.log("ktrv", ktrv)

    //create DerivationPath
    const path = new DerivationPath("m/1'");
    path.push(2, true);
    path.push(3, false);
    console.log(`path: ${path}`);

    // derive by path string
    console.log("xPrv1", xPrv.derivePath("m/1'/2'/3").intoString("xprv"))
    // derive by DerivationPath object
    console.log("xPrv3", xPrv.derivePath(path).intoString("xprv"))
    // create XPrv from ktrvxxx string and derive it
    console.log("xPrv2", XPrv.fromXPrv(ktrv).derivePath("m/1'/2'/3").intoString("xprv"))
    

    // get xpub
    let xPub = xPrv.toXPub();
    // derive xPub
    console.log("xPub", xPub.derivePath("m/1").intoString("xpub"));
    // get publicKey from xPub
    console.log("publicKey", xPub.toPublicKey().toString());
})();
