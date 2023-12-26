use crate::imports::*;
pub fn make_xpub() -> ExtendedPublicKeySecp256k1 {
    use kaspa_bip32::ExtendedKey;
    let xpub_base58 =
        "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
    let xpub = xpub_base58.parse::<ExtendedKey>().unwrap();
    xpub.try_into().unwrap()
}
