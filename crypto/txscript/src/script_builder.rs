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
    ElementExceedsMaxSize(usize),

    #[error("adding an integer would exceed the maximum allowed canonical script length of {MAX_SCRIPTS_SIZE}")]
    IntegerRejected,
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
/// ```
/// use txscript::opcodes::codes::*;
/// use txscript::script_builder::{Result, ScriptBuilder};
/// fn build_multisig_script(pub_key1: &[u8], pub_key2: &[u8], pub_key3: &[u8]) -> Result<Vec<u8>> {
///     ScriptBuilder::new()
///         .add_op(Op2)
///         .add_data(pub_key1).add_data(pub_key2).add_data(pub_key3)
///         .add_op(Op3)
///         .add_op(OpCheckMultiSig)
///         .drain()
/// }
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

    #[cfg(test)]
    pub fn raw_script(&self) -> &[u8] {
        &self.script
    }

    #[cfg(test)]
    pub fn raw_error(&self) -> Option<Error> {
        self.error
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
    /// A zero length buffer will lead to a push of empty data onto the stack (Op0 = OpFalse)
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
            self.error = Some(Error::ElementExceedsMaxSize(data_len));
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
        self.add_trimmed_u64(lock_time)
    }

    /// Gets a u64 sequence, converts it to byte array in little-endian, and then used the add_data function.
    pub fn add_sequence(&mut self, sequence: u64) -> &mut Self {
        self.add_trimmed_u64(sequence)
    }

    /// Gets a u64 lock time or sequence, converts it to byte array in little-endian, and then used the add_data function.
    fn add_trimmed_u64(&mut self, val: u64) -> &mut Self {
        if self.has_error() {
            return self;
        }

        let buffer: [u8; 8] = val.to_le_bytes();
        let trimmed_size = 8 - buffer.iter().rev().position(|x| *x != 0u8).unwrap_or(8);
        let trimmed = &buffer[0..trimmed_size];
        self.add_data(trimmed)
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*; 
    use std::iter::{once, repeat};

    // Tests that pushing opcodes to a script via the ScriptBuilder API works as expected.
    #[test]
    fn test_add_op() {
        struct Test {
            name: &'static str,
            opcodes: Vec<u8>,
            expected: Vec<u8>,
        }

        let tests = vec![
            Test { name: "push OP_FALSE", opcodes: vec![OpFalse], expected: vec![OpFalse] },
            Test { name: "push OP_TRUE", opcodes: vec![OpTrue], expected: vec![OpTrue] },
            Test { name: "push OP_0", opcodes: vec![Op0], expected: vec![Op0] },
            Test { name: "push OP_1 OP_2", opcodes: vec![Op1, Op2], expected: vec![Op1, Op2] },
            Test { name: "push OP_BLAKE2B OP_EQUAL", opcodes: vec![OpBlake2b, OpEqual], expected: vec![OpBlake2b, OpEqual] },
        ];

        // Run tests and individually add each op via AddOp.
        for test in tests.iter() {
            let mut builder = ScriptBuilder::new();
            test.opcodes.iter().for_each(|opcode| {
                builder.add_op(*opcode);
            });
            let result = builder.script().expect("the script is canonical");
            assert_eq!(result, &test.expected, "{} wrong result using add_op", test.name);
        }

        // Run tests and bulk add ops via AddOps.
        for test in tests.iter() {
            let mut builder = ScriptBuilder::new();
            let result = builder.add_ops(&test.opcodes).script().expect("the script is canonical");
            assert_eq!(result, &test.expected, "{} wrong result using add_ops", test.name);
        }
    }

    /// Tests that pushing signed integers to a script via the ScriptBuilder API works as expected.
    #[test]
    fn test_add_i64() {
        struct Test {
            name: &'static str,
            val: i64,
            expected: Vec<u8>,
        }

        let tests = vec![
            Test { name: "push -1", val: -1, expected: vec![Op1Negate] },
            Test { name: "push small int 0", val: 0, expected: vec![Op0] },
            Test { name: "push small int 1", val: 1, expected: vec![Op1] },
            Test { name: "push small int 2", val: 2, expected: vec![Op2] },
            Test { name: "push small int 3", val: 3, expected: vec![Op3] },
            Test { name: "push small int 4", val: 4, expected: vec![Op4] },
            Test { name: "push small int 5", val: 5, expected: vec![Op5] },
            Test { name: "push small int 6", val: 6, expected: vec![Op6] },
            Test { name: "push small int 7", val: 7, expected: vec![Op7] },
            Test { name: "push small int 8", val: 8, expected: vec![Op8] },
            Test { name: "push small int 9", val: 9, expected: vec![Op9] },
            Test { name: "push small int 10", val: 10, expected: vec![Op10] },
            Test { name: "push small int 11", val: 11, expected: vec![Op11] },
            Test { name: "push small int 12", val: 12, expected: vec![Op12] },
            Test { name: "push small int 13", val: 13, expected: vec![Op13] },
            Test { name: "push small int 14", val: 14, expected: vec![Op14] },
            Test { name: "push small int 15", val: 15, expected: vec![Op15] },
            Test { name: "push small int 16", val: 16, expected: vec![Op16] },
            Test { name: "push 17", val: 17, expected: vec![OpData1, 0x11] },
            Test { name: "push 65", val: 65, expected: vec![OpData1, 0x41] },
            Test { name: "push 127", val: 127, expected: vec![OpData1, 0x7f] },
            Test { name: "push 128", val: 128, expected: vec![OpData2, 0x80, 0] },
            Test { name: "push 255", val: 255, expected: vec![OpData2, 0xff, 0] },
            Test { name: "push 256", val: 256, expected: vec![OpData2, 0, 0x01] },
            Test { name: "push 32767", val: 32767, expected: vec![OpData2, 0xff, 0x7f] },
            Test { name: "push 32768", val: 32768, expected: vec![OpData3, 0, 0x80, 0] },
            Test { name: "push -2", val: -2, expected: vec![OpData1, 0x82] },
            Test { name: "push -3", val: -3, expected: vec![OpData1, 0x83] },
            Test { name: "push -4", val: -4, expected: vec![OpData1, 0x84] },
            Test { name: "push -5", val: -5, expected: vec![OpData1, 0x85] },
            Test { name: "push -17", val: -17, expected: vec![OpData1, 0x91] },
            Test { name: "push -65", val: -65, expected: vec![OpData1, 0xc1] },
            Test { name: "push -127", val: -127, expected: vec![OpData1, 0xff] },
            Test { name: "push -128", val: -128, expected: vec![OpData2, 0x80, 0x80] },
            Test { name: "push -255", val: -255, expected: vec![OpData2, 0xff, 0x80] },
            Test { name: "push -256", val: -256, expected: vec![OpData2, 0x00, 0x81] },
            Test { name: "push -32767", val: -32767, expected: vec![OpData2, 0xff, 0xff] },
            Test { name: "push -32768", val: -32768, expected: vec![OpData3, 0x00, 0x80, 0x80] },
        ];

        for test in tests {
            let mut builder = ScriptBuilder::new();
            let result = builder.add_i64(test.val).script().expect("the script is canonical");
            assert_eq!(result, test.expected, "{} wrong result", test.name);
        }
    }

    /// Tests that pushing data to a script via the ScriptBuilder API works as expected and conforms to BIP0062.
    #[test]
    fn test_add_data() {
        struct Test {
            name: &'static str,
            data: Vec<u8>,
            expected: Result<Vec<u8>>,
            /// use add_data_unchecked instead of add_data
            unchecked: bool,
        }

        let tests = vec![
            // BIP0062: Pushing an empty byte sequence must use OP_0.
            Test { name: "push empty byte sequence", data: vec![], expected: Ok(vec![Op0]), unchecked: false },
            Test { name: "push 1 byte 0x00", data: vec![0x00], expected: Ok(vec![Op0]), unchecked: false },
            // BIP0062: Pushing a 1-byte sequence of byte 0x01 through 0x10 must use OP_n.
            Test { name: "push 1 byte 0x01", data: vec![0x01], expected: Ok(vec![Op1]), unchecked: false },
            Test { name: "push 1 byte 0x02", data: vec![0x02], expected: Ok(vec![Op2]), unchecked: false },
            Test { name: "push 1 byte 0x03", data: vec![0x03], expected: Ok(vec![Op3]), unchecked: false },
            Test { name: "push 1 byte 0x04", data: vec![0x04], expected: Ok(vec![Op4]), unchecked: false },
            Test { name: "push 1 byte 0x05", data: vec![0x05], expected: Ok(vec![Op5]), unchecked: false },
            Test { name: "push 1 byte 0x06", data: vec![0x06], expected: Ok(vec![Op6]), unchecked: false },
            Test { name: "push 1 byte 0x07", data: vec![0x07], expected: Ok(vec![Op7]), unchecked: false },
            Test { name: "push 1 byte 0x08", data: vec![0x08], expected: Ok(vec![Op8]), unchecked: false },
            Test { name: "push 1 byte 0x09", data: vec![0x09], expected: Ok(vec![Op9]), unchecked: false },
            Test { name: "push 1 byte 0x0a", data: vec![0x0a], expected: Ok(vec![Op10]), unchecked: false },
            Test { name: "push 1 byte 0x0b", data: vec![0x0b], expected: Ok(vec![Op11]), unchecked: false },
            Test { name: "push 1 byte 0x0c", data: vec![0x0c], expected: Ok(vec![Op12]), unchecked: false },
            Test { name: "push 1 byte 0x0d", data: vec![0x0d], expected: Ok(vec![Op13]), unchecked: false },
            Test { name: "push 1 byte 0x0e", data: vec![0x0e], expected: Ok(vec![Op14]), unchecked: false },
            Test { name: "push 1 byte 0x0f", data: vec![0x0f], expected: Ok(vec![Op15]), unchecked: false },
            Test { name: "push 1 byte 0x10", data: vec![0x10], expected: Ok(vec![Op16]), unchecked: false },
            // BIP0062: Pushing the byte 0x81 must use OP_1NEGATE.
            Test { name: "push 1 byte 0x81", data: vec![0x81], expected: Ok(vec![Op1Negate]), unchecked: false },
            // BIP0062: Pushing any other byte sequence up to 75 bytes must
            // use the normal data push (opcode byte n, with n the number of
            // bytes, followed n bytes of data being pushed).
            Test { name: "push 1 byte 0x11", data: vec![0x11], expected: Ok(vec![OpData1, 0x11]), unchecked: false },
            Test { name: "push 1 byte 0x80", data: vec![0x80], expected: Ok(vec![OpData1, 0x80]), unchecked: false },
            Test { name: "push 1 byte 0x82", data: vec![0x82], expected: Ok(vec![OpData1, 0x82]), unchecked: false },
            Test { name: "push 1 byte 0xff", data: vec![0xff], expected: Ok(vec![OpData1, 0xff]), unchecked: false },
            Test {
                name: "push data len 17",
                data: vec![0x49; 17],
                expected: Ok(once(OpData17).chain(repeat(0x49).take(17)).collect()),
                unchecked: false,
            },
            Test {
                name: "push data len 75",
                data: vec![0x49; 75],
                expected: Ok(once(OpData75).chain(repeat(0x49).take(75)).collect()),
                unchecked: false,
            },
            // BIP0062: Pushing 76 to 255 bytes must use OP_PUSHDATA1.
            Test {
                name: "push data len 76",
                data: vec![0x49; 76],
                expected: Ok(once(OpPushData1).chain(once(76)).chain(repeat(0x49).take(76)).collect()),
                unchecked: false,
            },
            Test {
                name: "push data len 255",
                data: vec![0x49; 255],
                expected: Ok(once(OpPushData1).chain(once(255)).chain(repeat(0x49).take(255)).collect()),
                unchecked: false,
            },
            // // BIP0062: Pushing 256 to 520 bytes must use OP_PUSHDATA2.
            Test {
                name: "push data len 256",
                data: vec![0x49; 256],
                expected: Ok(once(OpPushData2).chain([0, 1]).chain(repeat(0x49).take(256)).collect()),
                unchecked: false,
            },
            Test {
                name: "push data len 520",
                data: vec![0x49; 520],
                expected: Ok(once(OpPushData2).chain([8, 2]).chain(repeat(0x49).take(520)).collect()),
                unchecked: false,
            },
            // BIP0062: OP_PUSHDATA4 can never be used, as pushes over 520
            // bytes are not allowed, and those below can be done using
            // other operators.
            Test {
                name: "push data len 521",
                data: vec![0x49; 521],
                expected: Err(Error::ElementExceedsMaxSize(521)),
                unchecked: false,
            },
            Test {
                name: "push data len 32767 (canonical)",
                data: vec![0x49; 32767],
                expected: Err(Error::DataRejected(32770)),
                unchecked: false,
            },
            Test {
                name: "push data len 65536 (canonical)",
                data: vec![0x49; 65536],
                expected: Err(Error::DataRejected(65541)),
                unchecked: false,
            },
            // // Additional tests for the add_data_unchecked function that
            // // intentionally allows data pushes to exceed the limit for
            // // testing purposes.

            // 3-byte data push via OP_PUSHDATA_2.
            Test {
                name: "push data len 32767 (non-canonical)",
                data: vec![0x49; 32767],
                expected: Ok(once(OpPushData2).chain([255, 127]).chain(repeat(0x49).take(32767)).collect()),
                unchecked: true,
            },
            // 5-byte data push via OP_PUSHDATA_4.
            Test {
                name: "push data len 65536 (non-canonical)",
                data: vec![0x49; 65536],
                expected: Ok(once(OpPushData4).chain([0, 0, 1, 0]).chain(repeat(0x49).take(65536)).collect()),
                unchecked: true,
            },
        ];

        for test in tests {
            let mut builder = ScriptBuilder::new();
            match test.unchecked {
                false => {
                    builder.add_data(&test.data);
                }
                true => {
                    builder.add_data_unchecked(&test.data);
                }
            }
            let result = builder.drain();
            assert_eq!(result, test.expected, "{} wrong result", test.name);
        }
    }

    #[test]
    fn test_trimmed_u64() {
        struct Test {
            name: &'static str,
            value: u64,
            expected: Result<Vec<u8>>,
        }

        let tests = vec![
            Test { name: "0x00", value: 0x00, expected: Ok(vec![Op0]) },
            Test { name: "0x01", value: 0x01, expected: Ok(vec![Op1]) },
            Test { name: "0xff", value: 0xff, expected: Ok(vec![OpData1, 0xff]) },
            Test { name: "0xffee", value: 0xffee, expected: Ok(vec![OpData2, 0xee, 0xff]) },
            Test { name: "0xffeedd", value: 0xffeedd, expected: Ok(vec![OpData3, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffeeddcc", value: 0xffeeddcc, expected: Ok(vec![OpData4, 0xcc, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffeeddccbb", value: 0xffeeddccbb, expected: Ok(vec![OpData5, 0xbb, 0xcc, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffeeddccbbaa", value: 0xffeeddccbbaa, expected: Ok(vec![OpData6, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffeeddccbbaa99", value: 0xffeeddccbbaa99, expected: Ok(vec![OpData7, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffeeddccbbaa9988", value: 0xffeeddccbbaa9988, expected: Ok(vec![OpData8, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]) },
            Test { name: "0xffffffffffffffff", value: u64::MAX, expected: Ok(once(OpData8).chain(repeat(0xff).take(8)).collect()), },
        ];

        for test in tests {
            let result = ScriptBuilder::new().add_trimmed_u64(test.value).drain();
            assert_eq!(result, test.expected, "{} wrong result", test.name);
            let result = ScriptBuilder::new().add_lock_time(test.value).drain();
            assert_eq!(result, test.expected, "{} wrong lock time result", test.name);
            let result = ScriptBuilder::new().add_sequence(test.value).drain();
            assert_eq!(result, test.expected, "{} wrong sequence result", test.name);
        }
    }

    /// Ensures that all of the functions that can be used to add data to a script don't allow
    /// the script to exceed the max allowed size.
    #[test]
    fn test_exceed_max_script_size() {
        fn full_builder() -> ScriptBuilder {
            let mut builder = ScriptBuilder::new();
            builder.add_data_unchecked(&[0u8; MAX_SCRIPTS_SIZE - 3]);
            builder
        }
        // Start off by constructing a max size script.
        let mut builder = full_builder();
        let original_result = builder.script().map(Vec::from);
        assert!(original_result.is_ok(), "adding unchecked data respecting the max script size must succeed");

        // Ensure adding data that would exceed the maximum size of the script
        // does not add the data.
        let result = builder.add_data(&[0u8]).script();
        assert_eq!(result, Err(Error::DataRejected(1)), "adding data that would exceed the maximum size of the script must fail");
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");

        // Ensure adding an opcode that would exceed the maximum size of the
        // script does not add the data.
        let result = full_builder().add_op(Op0).drain();
        assert_eq!(result, Err(Error::OpCodeRejected), "adding an opcode that would exceed the maximum size of the script must fail");
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");

        // Ensure adding an opcode array that would exceed the maximum size of the
        // script does not add the data.
        let result = full_builder().add_ops(&[OpCheckSig]).drain();
        assert_eq!(
            result,
            Err(Error::OpCodesRejected),
            "adding an opcode array that would exceed the maximum size of the script must fail"
        );
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");

        // Ensure adding an integer that would exceed the maximum size of the
        // script does not add the data.
        let result = full_builder().add_i64(0).drain();
        assert_eq!(
            result,
            Err(Error::IntegerRejected),
            "adding an integer that would exceed the maximum size of the script must fail"
        );
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");

        // Ensure adding a lock time that would exceed the maximum size of the
        // script does not add the data.
        let result = full_builder().add_lock_time(0).drain();
        assert_eq!(
            result,
            Err(Error::DataRejected(1)),
            "adding a lock time that would exceed the maximum size of the script must fail"
        );
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");

        // Ensure adding a sequence that would exceed the maximum size of the
        // script does not add the data.
        let result = full_builder().add_sequence(0).drain();
        assert_eq!(
            result,
            Err(Error::DataRejected(1)),
            "adding a sequence that would exceed the maximum size of the script must fail"
        );
        assert_eq!(builder.raw_script(), original_result.as_ref().unwrap(), "unexpected modified script");
    }
}
