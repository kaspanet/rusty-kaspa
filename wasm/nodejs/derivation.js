const kaspa = require('./kaspa/kaspa_wasm');
const {
    Mnemonic,
    XPrv,
    DerivationPath
} = kaspa;

kaspa.initConsolePanicHook();

(async () => {

    const mnemonic = Mnemonic.random();
    console.log("mnemonic:", mnemonic);
    const seed = mnemonic.toSeed("my_password");
    console.log("seed:", seed);

    // ---

    const xPrv = new XPrv(seed);
    console.log("xPrv", xPrv.intoString("xprv"))

    console.log("xPrv", xPrv.derivePath("m/1'/2'/3").intoString("xprv"))

    const path = new DerivationPath("m/1'");
    path.push(2, true);
    path.push(3, false);
    console.log(`path: ${path}`);

    console.log("xPrv", xPrv.derivePath(path).intoString("xprv"))

    const xPub = xPrv.publicKey();
    console.log("xPub", xPub.derivePath("m/1").intoString("xpub"));
})();
