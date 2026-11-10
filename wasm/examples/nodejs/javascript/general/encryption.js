const aero = require('../../../../nodejs/aero');

aero.initConsolePanicHook();

(async () => {

    let encrypted = aero.encryptXChaCha20Poly1305("my message", "my_password");
    console.log("encrypted:", encrypted);
    let decrypted = aero.decryptXChaCha20Poly1305(encrypted, "my_password");
    console.log("decrypted:", decrypted);

})();
