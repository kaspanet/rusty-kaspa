use secp256k1::{PublicKey, Secp256k1, SecretKey, XOnlyPublicKey};
use serde_wasm_bindgen::*;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone)]
#[wasm_bindgen(inspectable)]
pub struct Keypair {
    #[allow(dead_code)]
    // secret_key should not be exposed
    secret_key: SecretKey,
    public_key: PublicKey,
    xonly_public_key: XOnlyPublicKey,
}

#[wasm_bindgen]
impl Keypair {
    fn new(secret_key: SecretKey, public_key: PublicKey, xonly_public_key: XOnlyPublicKey) -> Self {
        Self { secret_key, public_key, xonly_public_key }
    }

    #[wasm_bindgen(getter = publicKey)]
    pub fn get_public_key(&self) -> JsValue {
        to_value(&self.public_key).unwrap()
    }

    #[wasm_bindgen(getter = xOnlyPublicKey)]
    pub fn get_xonly_public_key(&self) -> JsValue {
        to_value(&self.xonly_public_key).unwrap()
    }
}

#[wasm_bindgen]
pub fn generate_random_keypair_not_secure() -> Result<Keypair, JsError> {
    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    let (xonly_public_key, _) = public_key.x_only_public_key();
    Ok(Keypair::new(secret_key, public_key, xonly_public_key))
}
