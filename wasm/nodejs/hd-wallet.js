let kaspa = require('./kaspa/kaspa_wasm');
let {
    Mnemonic,
    XPrv,
    DerivationPath
} = kaspa;

kaspa.init_console_panic_hook();

(async ()=>{
    let mnemonic1 = Mnemonic.random();
    console.log("mnemonic1", mnemonic1);

    let mnemonic2 = new Mnemonic(mnemonic1.phrase);
    console.log("mnemonic2", mnemonic2);

    let seed1 = mnemonic1.toSeed("my_password");
    console.log("seed1", seed1);

    let seed2 = mnemonic2.toSeed("my_password");
    console.log("seed2", seed2);

    if (seed1 != seed2){
        throw Error("mnemonic restore dont works");
    }

    let xPrv = new XPrv(seed1);

    console.log("xPrv", xPrv.intoString("xprv"))

    console.log("xPrv", xPrv.derivePath("m/1'/2'/3").intoString("xprv"))

    let path = new DerivationPath("m/1'");
    path.push(2, true);
    path.push(3, false);
    console.log("path", path+"");

    console.log("xPrv", xPrv.derivePath(path).intoString("xprv"))

    let xPub = xPrv.publicKey();

    console.log("xPub", xPub.derivePath("m/1"));

})();