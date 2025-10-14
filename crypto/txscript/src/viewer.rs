use crate::{multi_sig::get_multisig_params, opcodes::codes, parse_script, TxScriptError};
use kaspa_consensus_core::{hashing::sighash::SigHashReusedValues, tx::VerifiableTransaction};
use std::{
    fmt::{Display, Formatter},
    marker::PhantomData,
    str,
};

pub struct ScriptViewer<'a, T, Reused> {
    script: &'a [u8],
    _phantom: PhantomData<(T, Reused)>,
}

impl<'a, T, Reused> ScriptViewer<'a, T, Reused>
where
    T: VerifiableTransaction,
    Reused: SigHashReusedValues,
{
    pub fn new(script: &'a [u8]) -> Self {
        Self { script, _phantom: PhantomData }
    }

    pub fn to_string(&self) -> Result<String, TxScriptError> {
        let opcodes: Vec<_> = parse_script::<T, Reused>(self.script).collect::<Result<_, _>>()?;
        let mut s = String::new();
        let mut indent_level: usize = 0;

        for (i, opcode) in opcodes.iter().enumerate() {
            let value = opcode.value();

            if value == codes::OpEndIf || value == codes::OpElse {
                indent_level = indent_level.saturating_sub(1);
            }

            s.push_str(&"  ".repeat(indent_level));

            let op_str = opcode_to_str(value);
            s.push_str(op_str);

            if value >= codes::OpData1 && value <= codes::OpData75 {
                let data = opcode.get_data();
                s.push(' ');
                s.push_str(&hex::encode(data));
            } else if value == codes::OpPushData1 || value == codes::OpPushData2 || value == codes::OpPushData4 {
                let data = opcode.get_data();
                s.push(' ');
                s.push_str(&data.len().to_string());
                s.push(' ');
                s.push_str(&hex::encode(data));

                // try to disassemble the data as a script
                let sub_viewer = ScriptViewer::<T, Reused>::new(data);
                if let Ok(sub_disassembly) = sub_viewer.to_string() {
                    if sub_disassembly.contains("OP_") {
                        s.push_str("\n    -- Begin Redeem Script --\n");
                        let indented = sub_disassembly.lines().map(|line| format!("{}", line)).collect::<Vec<_>>().join("\n");
                        s.push_str(&indented);
                        s.push_str("\n    -- End Redeem Script --");
                    }
                }
            } else if value == codes::OpCheckMultiSig || value == codes::OpCheckMultiSigVerify {
                let multisig_parameters = get_multisig_params(&opcodes, i)?;
                s.push_str(&format!(" // {} of {}", multisig_parameters.required_signatures_count, multisig_parameters.signers_count));
            }

            s.push('\n');

            if value == codes::OpIf || value == codes::OpNotIf || value == codes::OpElse {
                indent_level += 1;
            }
        }
        Ok(s)
    }
}

impl<T, Reused> Display for ScriptViewer<'_, T, Reused>
where
    T: VerifiableTransaction,
    Reused: SigHashReusedValues,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self.to_string() {
            Ok(s) => f.write_str(&s),
            Err(e) => write!(f, "Error disassembling script: {}", e),
        }
    }
}

#[allow(non_upper_case_globals)]
fn opcode_to_str(opcode: u8) -> &'static str {
    use crate::opcodes::codes::*;
    match opcode {
        OpFalse => "OP_FALSE",
        OpData1 => "OP_PUSHBYTES_1",
        OpData2 => "OP_PUSHBYTES_2",
        OpData3 => "OP_PUSHBYTES_3",
        OpData4 => "OP_PUSHBYTES_4",
        OpData5 => "OP_PUSHBYTES_5",
        OpData6 => "OP_PUSHBYTES_6",
        OpData7 => "OP_PUSHBYTES_7",
        OpData8 => "OP_PUSHBYTES_8",
        OpData9 => "OP_PUSHBYTES_9",
        OpData10 => "OP_PUSHBYTES_10",
        OpData11 => "OP_PUSHBYTES_11",
        OpData12 => "OP_PUSHBYTES_12",
        OpData13 => "OP_PUSHBYTES_13",
        OpData14 => "OP_PUSHBYTES_14",
        OpData15 => "OP_PUSHBYTES_15",
        OpData16 => "OP_PUSHBYTES_16",
        OpData17 => "OP_PUSHBYTES_17",
        OpData18 => "OP_PUSHBYTES_18",
        OpData19 => "OP_PUSHBYTES_19",
        OpData20 => "OP_PUSHBYTES_20",
        OpData21 => "OP_PUSHBYTES_21",
        OpData22 => "OP_PUSHBYTES_22",
        OpData23 => "OP_PUSHBYTES_23",
        OpData24 => "OP_PUSHBYTES_24",
        OpData25 => "OP_PUSHBYTES_25",
        OpData26 => "OP_PUSHBYTES_26",
        OpData27 => "OP_PUSHBYTES_27",
        OpData28 => "OP_PUSHBYTES_28",
        OpData29 => "OP_PUSHBYTES_29",
        OpData30 => "OP_PUSHBYTES_30",
        OpData31 => "OP_PUSHBYTES_31",
        OpData32 => "OP_PUSHBYTES_32",
        OpData33 => "OP_PUSHBYTES_33",
        OpData34 => "OP_PUSHBYTES_34",
        OpData35 => "OP_PUSHBYTES_35",
        OpData36 => "OP_PUSHBYTES_36",
        OpData37 => "OP_PUSHBYTES_37",
        OpData38 => "OP_PUSHBYTES_38",
        OpData39 => "OP_PUSHBYTES_39",
        OpData40 => "OP_PUSHBYTES_40",
        OpData41 => "OP_PUSHBYTES_41",
        OpData42 => "OP_PUSHBYTES_42",
        OpData43 => "OP_PUSHBYTES_43",
        OpData44 => "OP_PUSHBYTES_44",
        OpData45 => "OP_PUSHBYTES_45",
        OpData46 => "OP_PUSHBYTES_46",
        OpData47 => "OP_PUSHBYTES_47",
        OpData48 => "OP_PUSHBYTES_48",
        OpData49 => "OP_PUSHBYTES_49",
        OpData50 => "OP_PUSHBYTES_50",
        OpData51 => "OP_PUSHBYTES_51",
        OpData52 => "OP_PUSHBYTES_52",
        OpData53 => "OP_PUSHBYTES_53",
        OpData54 => "OP_PUSHBYTES_54",
        OpData55 => "OP_PUSHBYTES_55",
        OpData56 => "OP_PUSHBYTES_56",
        OpData57 => "OP_PUSHBYTES_57",
        OpData58 => "OP_PUSHBYTES_58",
        OpData59 => "OP_PUSHBYTES_59",
        OpData60 => "OP_PUSHBYTES_60",
        OpData61 => "OP_PUSHBYTES_61",
        OpData62 => "OP_PUSHBYTES_62",
        OpData63 => "OP_PUSHBYTES_63",
        OpData64 => "OP_PUSHBYTES_64",
        OpData65 => "OP_PUSHBYTES_65",
        OpData66 => "OP_PUSHBYTES_66",
        OpData67 => "OP_PUSHBYTES_67",
        OpData68 => "OP_PUSHBYTES_68",
        OpData69 => "OP_PUSHBYTES_69",
        OpData70 => "OP_PUSHBYTES_70",
        OpData71 => "OP_PUSHBYTES_71",
        OpData72 => "OP_PUSHBYTES_72",
        OpData73 => "OP_PUSHBYTES_73",
        OpData74 => "OP_PUSHBYTES_74",
        OpData75 => "OP_PUSHBYTES_75",
        OpPushData1 => "OP_PUSHDATA1",
        OpPushData2 => "OP_PUSHDATA2",
        OpPushData4 => "OP_PUSHDATA4",
        Op1Negate => "OP_1NEGATE",
        OpReserved => "OP_RESERVED",
        OpTrue => "OP_1",
        Op2 => "OP_2",
        Op3 => "OP_3",
        Op4 => "OP_4",
        Op5 => "OP_5",
        Op6 => "OP_6",
        Op7 => "OP_7",
        Op8 => "OP_8",
        Op9 => "OP_9",
        Op10 => "OP_10",
        Op11 => "OP_11",
        Op12 => "OP_12",
        Op13 => "OP_13",
        Op14 => "OP_14",
        Op15 => "OP_15",
        Op16 => "OP_16",
        OpNop => "OP_NOP",
        OpVer => "OP_VER",
        OpIf => "OP_IF",
        OpNotIf => "OP_NOTIF",
        OpVerIf => "OP_VERIF",
        OpVerNotIf => "OP_VERNOTIF",
        OpElse => "OP_ELSE",
        OpEndIf => "OP_ENDIF",
        OpVerify => "OP_VERIFY",
        OpReturn => "OP_RETURN",
        OpToAltStack => "OP_TOALTSTACK",
        OpFromAltStack => "OP_FROMALTSTACK",
        Op2Drop => "OP_2DROP",
        Op2Dup => "OP_2DUP",
        Op3Dup => "OP_3DUP",
        Op2Over => "OP_2OVER",
        Op2Rot => "OP_2ROT",
        Op2Swap => "OP_2SWAP",
        OpIfDup => "OP_IFDUP",
        OpDepth => "OP_DEPTH",
        OpDrop => "OP_DROP",
        OpDup => "OP_DUP",
        OpNip => "OP_NIP",
        OpOver => "OP_OVER",
        OpPick => "OP_PICK",
        OpRoll => "OP_ROLL",
        OpRot => "OP_ROT",
        OpSwap => "OP_SWAP",
        OpTuck => "OP_TUCK",
        OpCat => "OP_CAT",
        OpSubStr => "OP_SUBSTR",
        OpLeft => "OP_LEFT",
        OpRight => "OP_RIGHT",
        OpSize => "OP_SIZE",
        OpInvert => "OP_INVERT",
        OpAnd => "OP_AND",
        OpOr => "OP_OR",
        OpXor => "OP_XOR",
        OpEqual => "OP_EQUAL",
        OpEqualVerify => "OP_EQUALVERIFY",
        OpReserved1 => "OP_RESERVED1",
        OpReserved2 => "OP_RESERVED2",
        Op1Add => "OP_1ADD",
        Op1Sub => "OP_1SUB",
        Op2Mul => "OP_2MUL",
        Op2Div => "OP_2DIV",
        OpNegate => "OP_NEGATE",
        OpAbs => "OP_ABS",
        OpNot => "OP_NOT",
        Op0NotEqual => "OP_0NOTEQUAL",
        OpAdd => "OP_ADD",
        OpSub => "OP_SUB",
        OpMul => "OP_MUL",
        OpDiv => "OP_DIV",
        OpMod => "OP_MOD",
        OpLShift => "OP_LSHIFT",
        OpRShift => "OP_RSHIFT",
        OpBoolAnd => "OP_BOOLAND",
        OpBoolOr => "OP_BOOLOR",
        OpNumEqual => "OP_NUMEQUAL",
        OpNumEqualVerify => "OP_NUMEQUALVERIFY",
        OpNumNotEqual => "OP_NUMNOTEQUAL",
        OpLessThan => "OP_LESSTHAN",
        OpGreaterThan => "OP_GREATERTHAN",
        OpLessThanOrEqual => "OP_LESSTHANOREQUAL",
        OpGreaterThanOrEqual => "OP_GREATERTHANOREQUAL",
        OpMin => "OP_MIN",
        OpMax => "OP_MAX",
        OpWithin => "OP_WITHIN",
        OpSHA256 => "OP_SHA256",
        OpCheckMultiSigECDSA => "OP_CHECKMULTISIGECDSA",
        OpBlake2b => "OP_BLAKE2B",
        OpCheckSigECDSA => "OP_CHECKSIGECDSA",
        OpCheckSig => "OP_CHECKSIG",
        OpCheckSigVerify => "OP_CHECKSIGVERIFY",
        OpCheckMultiSig => "OP_CHECKMULTISIG",
        OpCheckMultiSigVerify => "OP_CHECKMULTISIGVERIFY",
        OpCheckLockTimeVerify => "OP_CHECKLOCKTIMEVERIFY",
        OpCheckSequenceVerify => "OP_CHECKSEQUENCEVERIFY",
        OpTxInputCount => "OP_TXINPUTCOUNT",
        OpTxOutputCount => "OP_TXOUTPUTCOUNT",
        OpTxInputIndex => "OP_TXINPUTINDEX",
        OpTxInputAmount => "OP_TXINPUTAMOUNT",
        OpTxInputSpk => "OP_TXINPUTSPK",
        OpTxOutputAmount => "OP_TXOUTPUTAMOUNT",
        OpTxOutputSpk => "OP_TXOUTPUTSPK",
        _ => "OP_UNKNOWN",
    }
}
