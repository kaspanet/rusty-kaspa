use std::iter::once;

use crate::{data_stack::OpcodeData, opcodes::codes::*, MAX_SCRIPTS_SIZE, MAX_SCRIPT_ELEMENT_SIZE};
use thiserror::Error;

/// DEFAULT_SCRIPT_ALLOC is the default size used for the backing array
/// for a script being built by the ScriptBuilder. The array will
/// dynamically grow as needed, but this figure is intended to provide
/// enough space for vast majority of scripts without needing to grow the
/// backing array multiple times.
const DEFAULT_SCRIPT_ALLOC: usize = 512;

#[derive(Error, PartialEq, Eq, Debug, Clone, Copy)]
pub enum Error {
    #[error("adding an opcode would exceed the maximum allowed canonical script length of {MAX_SCRIPTS_SIZE}")]
    OpCodeRejected,

    #[error("adding opcodes would exceed the maximum allowed canonical script length of {MAX_SCRIPTS_SIZE}")]
    OpCodesRejected,

    #[error("adding {0} bytes of data would exceed the maximum allowed canonical script length of {MAX_SCRIPTS_SIZE}")]
    DataRejected(usize),

    #[error("adding a data element of {0} bytes exceed the maximum allowed script element size of {MAX_SCRIPT_ELEMENT_SIZE}")]
    ElementRejected(usize),

    #[error("adding an integer would exceed the maximum allowed canonical script length of {MAX_SCRIPTS_SIZE}")]
    IntegerRejected,

    #[error("adding the unsigned integer {0} would exceed the maximum allowed encoded length of 8")]
    NumberIsTooBig(u64),
}
pub type Result<T> = std::result::Result<T, Error>;

/// ScriptBuilder provides a facility for building custom scripts. It allows
/// you to push opcodes, ints, and data while respecting canonical encoding. In
/// general it does not ensure the script will execute correctly, however any
/// data pushes which would exceed the maximum allowed script engine limits and
/// are therefore guaranteed not to execute will not be pushed and will result in
/// the Script function returning an error.
///
/// For example, the following would build a 2-of-3 multisig script for usage in
/// a pay-to-script-hash (although in this situation MultiSigScript() would be a
/// better choice to generate the script):
///
/// FIXME: rust code here
/// ```
///     let builder := ScriptBuilder::new()
///     builder.AddOp(Op2).AddData(pubKey1).AddData(pubKey2)
///     builder.AddData(pubKey3).AddOp(Op3)
///     builder.AddOp(OpCheckMultiSig)
///     let script = builder.Script()
///     if let Some(err) = script {
///         // Handle the error.
///         return;
///     }
///     trace!("Final multi-sig script: {:?}", script.unwrap());
/// ```
pub struct ScriptBuilder {
    script: Vec<u8>,
    error: Option<Error>,
}

impl ScriptBuilder {
    pub fn new() -> Self {
        Self { script: Vec::with_capacity(DEFAULT_SCRIPT_ALLOC), error: None }
    }

    pub fn script(&self) -> Result<&[u8]> {
        match self.error {
            None => Ok(&self.script),
            Some(ref err) => Err(*err),
        }
    }

    pub fn drain(&mut self) -> Result<Vec<u8>> {
        match self.error {
            None => Ok(std::mem::take(&mut self.script)),
            Some(err) => {
                self.script = vec![];
                self.error = None;
                Err(err)
            }
        }
    }

    #[inline(always)]
    fn has_error(&self) -> bool {
        self.error.is_some()
    }

    /// Pushes the passed opcode to the end of the script. The script will not
    /// be modified if pushing the opcode would cause the script to exceed the
    /// maximum allowed script engine size.
    pub fn add_op(&mut self, opcode: u8) -> &mut Self {
        if self.has_error() {
            return self;
        }

        // Pushes that would cause the script to exceed the largest allowed
        // script size would result in a non-canonical script.
        if self.script.len() >= MAX_SCRIPTS_SIZE {
            self.error = Some(Error::OpCodeRejected);
            return self;
        }

        self.script.push(opcode);
        self
    }

    pub fn add_ops(&mut self, opcodes: &[u8]) -> &mut Self {
        if self.has_error() {
            return self;
        }

        // Pushes that would cause the script to exceed the largest allowed
        // script size would result in a non-canonical script.
        if self.script.len() + opcodes.len() > MAX_SCRIPTS_SIZE {
            self.error = Some(Error::OpCodesRejected);
            return self;
        }

        self.script.extend_from_slice(opcodes);
        self
    }

    /// Returns the number of bytes the canonical encoding of the data will take.
    pub fn canonical_data_size(data: &[u8]) -> usize {
        let data_len = data.len();

        // When the data consists of a single number that can be represented
        // by one of the "small integer" opcodes, that opcode will be instead
        // of a data push opcode followed by the number.
        if data_len == 0 || (data_len == 1 && (data[0] <= 16 || data[0] == 0x81)) {
            return 1;
        }

        data_len
            + if data_len < OpPushData1 as usize {
                1
            } else if data_len <= 0xff {
                2
            } else if data_len <= 0xffff {
                3
            } else {
                5
            }
    }

    /// Internal function that actually pushes the passed data to the
    /// end of the script. It automatically chooses canonical opcodes depending on
    /// the length of the data. A zero length buffer will lead to a push of empty
    /// data onto the stack (OP_0). No data limits are enforced with this function.
    fn add_raw_data(&mut self, data: &[u8]) -> &mut Self {
        let data_len = data.len();

        // When the data consists of a single number that can be represented
        // by one of the "small integer" opcodes, use that opcode instead of
        // a data push opcode followed by the number.
        if data_len == 0 || data_len == 1 && data[0] == 0 {
            self.script.push(Op0);
            return self;
        } else if data_len == 1 && data[0] <= 16 {
            self.script.push((Op1 - 1) + data[0]);
            return self;
        } else if data_len == 1 && data[0] == 0x81 {
            self.script.push(Op1Negate);
            return self;
        }

        // Use one of the OpData# opcodes if the length of the data is small
        // enough so the data push instruction is only a single byte.
        // Otherwise, choose the smallest possible OpPushData# opcode that
        // can represent the length of the data.
        if data_len < OpPushData1 as usize {
            self.script.push(OpData1 - 1 + data_len as u8);
        } else if data_len <= 0xff {
            self.script.extend(once(OpPushData1).chain(once(data_len as u8)));
        } else if data_len <= 0xffff {
            self.script.extend(once(OpPushData2).chain((data_len as u16).to_le_bytes()));
        } else {
            self.script.extend(once(OpPushData4).chain((data_len as u32).to_le_bytes()));
        }

        // Append the actual data.
        self.script.extend(data);
        self
    }

    /// This function should not typically be used by ordinary users as it does not
    /// include the checks which prevent data pushes larger than the maximum allowed
    /// sizes which leads to scripts that can't be executed. This is provided for
    /// testing purposes such as tests where sizes are intentionally made larger
    /// than allowed.
    ///
    /// Use add_data instead.
    #[cfg(test)]
    pub fn add_data_unchecked(&mut self, data: &[u8]) -> &mut Self {
        if self.has_error() {
            return self;
        }

        self.add_raw_data(data)
    }

    /// AddData pushes the passed data to the end of the script. It automatically
    /// chooses canonical opcodes depending on the length of the data.
    ///
    /// A zero length buffer will lead to a push of empty data onto the stack (Op0 = [`OpFalse`])
    /// and any push of data greater than [`MAX_SCRIPT_ELEMENT_SIZE`] will not modify
    /// the script since that is not allowed by the script engine.
    ///
    /// Also, the script will not be modified if pushing the data would cause the script to
    /// exceed the maximum allowed script engine size [`MAX_SCRIPTS_SIZE`].
    pub fn add_data(&mut self, data: &[u8]) -> &mut Self {
        if self.has_error() {
            return self;
        }

        // Pushes that would cause the script to exceed the largest allowed
        // script size would result in a non-canonical script.
        let data_size = Self::canonical_data_size(data);

        if self.script.len() + data_size > MAX_SCRIPTS_SIZE {
            self.error = Some(Error::DataRejected(data_size));
            return self;
        }

        // Pushes larger than the max script element size would result in a
        // script that is not canonical.
        let data_len = data.len();
        if data_len > MAX_SCRIPT_ELEMENT_SIZE {
            self.error = Some(Error::ElementRejected(data_len));
            return self;
        }

        self.add_raw_data(data)
    }

    pub fn add_i64(&mut self, val: i64) -> &mut Self {
        if self.has_error() {
            return self;
        }

        // Pushes that would cause the script to exceed the largest allowed
        // script size would result in a non-canonical script.
        if self.script.len() + 1 > MAX_SCRIPTS_SIZE {
            self.error = Some(Error::IntegerRejected);
            return self;
        }

        // Fast path for small integers and Op1Negate.
        if val == 0 {
            self.script.push(Op0);
            return self;
        }
        if val == -1 || (1..=16).contains(&val) {
            self.script.push(((Op1 as i64 - 1) + val) as u8);
            return self;
        }

        let bytes: Vec<_> = OpcodeData::serialize(&val);
        self.add_data(&bytes)
    }

    /// Gets a u64 lock time, converts it to byte array in little-endian, and then used the add_data function.
    pub fn add_lock_time(&mut self, lock_time: u64) -> &mut Self {
        self.add_u64_as_i64(lock_time)
    }

    /// Gets a u64 sequence, converts it to byte array in little-endian, and then used the add_data function.
    pub fn add_sequence(&mut self, sequence: u64) -> &mut Self {
        self.add_u64_as_i64(sequence)
    }

    /// Gets a u64 lock time or sequence, converts it to byte array in little-endian, and then used the add_data function.
    fn add_u64_as_i64(&mut self, val: u64) -> &mut Self {
        if self.has_error() {
            return self;
        }

        // TODO: This implementation differs from golang. Verify it is functionally equivalent.
        let converted = <i64>::try_from(val);
        if converted.is_err() {
            self.error = Some(Error::NumberIsTooBig(val));
            return self;
        }

        let bytes: Vec<_> = OpcodeData::serialize(converted.as_ref().unwrap());
        self.add_data(&bytes)
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self::new()
    }
}
