const kaspa = require('../../../../nodejs/kaspa');

kaspa.initConsolePanicHook();

(async () => {

    let encrypted = kaspa.encryptXChaCha20Poly1305("my message", "my_password");
    console.log("encrypted:", encrypted);
    let decrypted = kaspa.decryptXChaCha20Poly1305(encrypted, "my_password");
    console.log("decrypted:", decrypted);

})();
