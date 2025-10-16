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

    pub fn try_to_string(&self) -> Result<String, TxScriptError> {
        let opcodes: Vec<_> = parse_script::<T, Reused>(self.script).collect::<Result<_, _>>()?;
        let mut s = String::new();
        let mut indent_level: usize = 0;

        for (i, opcode) in opcodes.iter().enumerate() {
            let value = opcode.value();

            if value == codes::OpEndIf || value == codes::OpElse {
                indent_level = indent_level.saturating_sub(1);
            }

            s.push_str(&"  ".repeat(indent_level));

            s.push_str(opcode.to_string().as_str());

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
                if let Ok(sub_disassembly) = sub_viewer.try_to_string() {
                    if sub_disassembly.contains("OP_") {
                        s.push_str("\n    -- Begin Redeem Script --\n");
                        let indented = sub_disassembly.lines().map(|line| format!("{}", line)).collect::<Vec<_>>().join("\n");
                        s.push_str(&indented);
                        s.push_str("\n    -- End Redeem Script --");
                    }
                }
            } else if value == codes::OpCheckMultiSig || value == codes::OpCheckMultiSigVerify {
                let multisig_parameters = get_multisig_params(&opcodes, i)?;
                s.push_str(&format!(
                    "\n// {} of {}",
                    multisig_parameters.required_signatures_count, multisig_parameters.signers_count
                ));

                for pubkey in multisig_parameters.signers_pubkey.iter() {
                    s.push_str(&format!("\n// {}", bs58::encode(pubkey.serialize()).into_string()));
                }
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
        match self.try_to_string() {
            Ok(s) => f.write_str(&s),
            Err(e) => write!(f, "Error disassembling script: {}", e),
        }
    }
}
