#[derive(PartialEq, Eq)]
pub enum ScriptClass {
    /// None of the recognized forms.
    NonStandard = 0,

    /// Pay to pubkey.
    PubKey = 1,

    /// Pay to pubkey ECDSA.
    _PubKeyECDSA = 2,

    /// Pay to script hash.
    ScriptHash = 3,
}
