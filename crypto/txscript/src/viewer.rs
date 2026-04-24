use crate::{TxScriptError, multi_sig::get_schnorr_multisig_params, opcodes::codes, parse_script};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::{hashing::sighash::SigHashReusedValues, tx::VerifiableTransaction};
use std::{
    fmt::{Display, Formatter},
    marker::PhantomData,
};

#[derive(Debug, Clone)]
pub struct ScriptViewerOptions {
    /// if true, viewer tries to dissassemble a sub-script
    pub contains_redeem_script: bool,
    /// prefix used when formatting Kaspa addresses
    pub address_prefix: Prefix,
}

impl Default for ScriptViewerOptions {
    fn default() -> Self {
        Self { contains_redeem_script: false, address_prefix: Prefix::Mainnet }
    }
}

pub struct ScriptViewer<'a, T, Reused> {
    script: &'a [u8],
    options: ScriptViewerOptions,
    _phantom: PhantomData<(T, Reused)>,
}

impl<'a, T, Reused> ScriptViewer<'a, T, Reused>
where
    T: VerifiableTransaction,
    Reused: SigHashReusedValues,
{
    pub fn new(script: &'a [u8], options: ScriptViewerOptions) -> Self {
        Self { script, options, _phantom: PhantomData }
    }

    pub fn try_to_string(&self) -> Result<String, TxScriptError> {
        let opcodes: Vec<_> = parse_script::<T, Reused>(self.script).collect::<Result<_, _>>()?;
        let mut s = String::new();
        let mut indent_level: usize = 0;

        for (i, opcode) in opcodes.iter().enumerate() {
            let value = opcode.value();
            let opcode_display = opcode.to_string();
            let opcode_name = opcode_display.split_whitespace().next().unwrap_or(opcode_display.as_str());

            if value == codes::OpEndIf || value == codes::OpElse {
                indent_level = indent_level.saturating_sub(1);
            }

            let current_indent = "  ".repeat(indent_level);
            s.push_str(&current_indent);

            if (codes::OpData1..=codes::OpData75).contains(&value) {
                let data = opcode.get_data();
                s.push_str(opcode_name);
                s.push_str(" hex: ");
                s.push_str(&faster_hex::hex_string(data));
            } else if value == codes::OpPushData1 || value == codes::OpPushData2 || value == codes::OpPushData4 {
                let data = opcode.get_data();
                s.push_str(opcode_name);
                s.push_str(" len=");
                s.push_str(&data.len().to_string());
                s.push_str(" hex: ");
                s.push_str(&faster_hex::hex_string(data));

                if self.options.contains_redeem_script {
                    // Try to disassemble the pushed data as a redeem script and keep it visually tied to the hex blob.
                    let sub_viewer = ScriptViewer::<T, Reused>::new(
                        data,
                        ScriptViewerOptions { contains_redeem_script: false, address_prefix: self.options.address_prefix },
                    );
                    if let Ok(sub_disassembly) = sub_viewer.try_to_string()
                        && sub_disassembly.contains("Op")
                    {
                        s.push('\n');
                        let nested_indent = format!("{current_indent}  ");
                        s.push_str(&nested_indent);
                        s.push_str("(redeem script:\n");
                        let script_indent = format!("{nested_indent}  ");
                        let indented =
                            sub_disassembly.lines().map(|line| format!("{script_indent}{line}")).collect::<Vec<_>>().join("\n");
                        s.push_str(&indented);
                        s.push('\n');
                        s.push_str(&nested_indent);
                        s.push(')');
                    }
                }
            } else if value == codes::OpCheckMultiSig {
                s.push_str(&opcode_display);
                let multisig_parameters = get_schnorr_multisig_params(&opcodes, i)?;
                s.push_str(&format!(
                    "\n// {} of {}",
                    multisig_parameters.required_signatures_count, multisig_parameters.signers_count
                ));

                for pubkey in multisig_parameters.signers_pubkey.iter() {
                    let address = Address::new(self.options.address_prefix, Version::PubKey, pubkey.serialize().as_slice());
                    s.push_str(&format!("\n// {address}"));
                }
            } else {
                s.push_str(&opcode_display);
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

#[cfg(test)]
mod tests {
    use super::ScriptViewerOptions;
    use crate::script_builder::ScriptBuilder;
    use kaspa_consensus_core::{hashing::sighash::SigHashReusedValuesSync, tx::ValidatedTransaction};

    const MULTISIG_SIGNATURE_SCRIPT_HEX: &str = "4130ef124590e4e6627078a658e2eb0b89fe4733f40d8cbfe0d077ae16bb90afb0a5f10e5693352e4b9d19d77a98fe75e395ce60988a0750ab8603a252c9c7290401412294d292317d03d1a5f49a8204c35486da84bbbea604209637e2bbfb5bbfabb36bcb37fc90aeb9836ed950a42b87382880fbd926b362cdbca16e9db9891918850141c2d76d4c64c9b8a8a64fa34a69f7cea953c4f0e564463226d931481ee1fbccafd7c20500a699fc8a10d01d03219d25944081750cdbba89e6a5a64b3224f58a5a014c875320b0a2f302b97271d6d1f20f2168e8b86b037d42a52aaf7ca959bea8a8bbf859a220e040996f44024491881ad4d2f59d4397a5a1f2e169c55624cb9509693fbb7a14204e518f0ecb51eef7db45042e441bb4d99f2c68277359bea369fcb7c80bee5b0120924013135715c9a8076141a33d6528a13fa2e816d3f006897b6d6c8b1da90fd754ae";

    #[test]
    fn string_view_prints_multisig_parameters_for_signature_script_redeem_script() {
        let script = hex::decode(MULTISIG_SIGNATURE_SCRIPT_HEX).unwrap();
        let mut builder = ScriptBuilder::new();
        builder.script_mut().extend_from_slice(&script);

        let view = builder.string_view::<ValidatedTransaction, SigHashReusedValuesSync>(ScriptViewerOptions {
            contains_redeem_script: true,
            ..Default::default()
        });

        assert!(view.contains("OpPushData1 len=135 hex:"));
        assert!(view.contains("(redeem script:"));
        assert!(view.contains("OpCheckMultiSig"));
        assert!(view.contains("// 3 of 4"));
        assert!(view.contains("kaspa:"));
    }
}
