use kaspa_txscript::opcodes::codes::{Op0, OpDrop, OpEqualVerify, OpInputCovenantId, OpTrue, OpTxInputScriptSigSubstr};
use kaspa_txscript::script_builder::ScriptBuilder;

/// Script domain prefixes for distinguishing different scripts within the same covenant.
///
/// Each domain has a 2-byte prefix (`[opcode, OP_DROP]`) prepended to both the redeem script
/// and the sig_script. This creates distinct SPKs per script purpose and enables
/// cross-script verification via sig_script prefix checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptDomain {
    /// State verification (rollup): `[OP_0(0x00), OP_DROP(0x75)]`
    StateVerification,
    /// Permission: `[OP_1(0x51), OP_DROP(0x75)]`
    Permission,
}

impl ScriptDomain {
    /// Get the raw prefix bytes for this domain.
    pub fn prefix_bytes(&self) -> [u8; 2] {
        match self {
            ScriptDomain::StateVerification => [Op0, OpDrop],
            ScriptDomain::Permission => [OpTrue, OpDrop], // OP_1 == OpTrue == 0x51
        }
    }
}

/// Build the permission redeem script.
///
/// Format: `[OP_1(0x51), OP_DROP(0x75), OP_TRUE(0x51)]`
/// - Domain-tagged with Permission prefix
/// - Always succeeds (OP_TRUE at end)
pub fn build_permission_redeem_script() -> Vec<u8> {
    ScriptBuilder::new()
        .add_op(OpTrue)
        .unwrap() // domain tag byte 1 (0x51)
        .add_op(OpDrop)
        .unwrap() // domain tag byte 2 (0x75)
        .add_op(OpTrue)
        .unwrap() // always succeed
        .drain()
}

/// Build the sig_script for the permission script.
///
/// Pushes the permission redeem script for P2SH execution.
pub fn build_permission_sig_script(permission_redeem: &[u8]) -> Vec<u8> {
    ScriptBuilder::new().add_data(permission_redeem).unwrap().drain()
}

/// Build the delegate/entry script.
///
/// This script verifies:
/// 1. Input 0 has the expected covenant_id (permission covenant)
/// 2. Input 0's sig_script starts with `[0x51, 0x75]` (permission domain tag)
/// 3. Returns OP_TRUE
///
/// This allows a delegate input to verify that the permission input (input 0)
/// is using the correct permission script from the correct covenant.
pub fn build_delegate_entry_script(permission_covenant_id: &[u8; 32]) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // Check 1: Input 0 has expected covenant_id
    // push 0 → OpInputCovenantId → push expected_id → OpEqualVerify
    builder.add_op(Op0).unwrap();
    builder.add_op(OpInputCovenantId).unwrap();
    builder.add_data(permission_covenant_id).unwrap();
    builder.add_op(OpEqualVerify).unwrap();

    // Check 2: Input 0's sig_script starts with [0x51, 0x75] (permission domain tag)
    // push 0 → push 0 → push 2 → OpTxInputScriptSigSubstr → push [0x51, 0x75] → OpEqualVerify
    builder.add_op(Op0).unwrap(); // input index
    builder.add_op(Op0).unwrap(); // start offset
    builder.add_i64(2).unwrap(); // length
    builder.add_op(OpTxInputScriptSigSubstr).unwrap();
    builder.add_data(&ScriptDomain::Permission.prefix_bytes()).unwrap();
    builder.add_op(OpEqualVerify).unwrap();

    // Success
    builder.add_op(OpTrue).unwrap();

    builder.drain()
}
