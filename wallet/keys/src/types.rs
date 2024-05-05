//!
//!  Type aliases used by the wallet framework.
//!

use std::sync::Arc;

pub type ExtendedPublicKeySecp256k1 = kaspa_bip32::ExtendedPublicKey<secp256k1::PublicKey>;

pub type ExtendedPublicKeys = Arc<Vec<ExtendedPublicKeySecp256k1>>;
