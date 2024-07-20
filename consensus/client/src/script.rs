use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use kaspa_wasm_core::types::{BinaryT, HexString};

use crate::imports::*;
use crate::result::Result;
use kaspa_txscript::script_builder as native;

#[wasm_bindgen(typescript_custom_section)]
const TS_SCRIPT_OPCODES: &'static str = r#"
/**
 * Kaspa Transaction Script Opcodes
 * @see {@link ScriptBuilder}
 * @category Consensus
 */
export enum Opcode {
    OpData1 = 0x01,
    OpData2 = 0x02,
    OpData3 = 0x03,
    OpData4 = 0x04,
    OpData5 = 0x05,
    OpData6 = 0x06,
    OpData7 = 0x07,
    OpData8 = 0x08,
    OpData9 = 0x09,
    OpData10 = 0x0a,
    OpData11 = 0x0b,
    OpData12 = 0x0c,
    OpData13 = 0x0d,
    OpData14 = 0x0e,
    OpData15 = 0x0f,
    OpData16 = 0x10,
    OpData17 = 0x11,
    OpData18 = 0x12,
    OpData19 = 0x13,
    OpData20 = 0x14,
    OpData21 = 0x15,
    OpData22 = 0x16,
    OpData23 = 0x17,
    OpData24 = 0x18,
    OpData25 = 0x19,
    OpData26 = 0x1a,
    OpData27 = 0x1b,
    OpData28 = 0x1c,
    OpData29 = 0x1d,
    OpData30 = 0x1e,
    OpData31 = 0x1f,
    OpData32 = 0x20,
    OpData33 = 0x21,
    OpData34 = 0x22,
    OpData35 = 0x23,
    OpData36 = 0x24,
    OpData37 = 0x25,
    OpData38 = 0x26,
    OpData39 = 0x27,
    OpData40 = 0x28,
    OpData41 = 0x29,
    OpData42 = 0x2a,
    OpData43 = 0x2b,
    OpData44 = 0x2c,
    OpData45 = 0x2d,
    OpData46 = 0x2e,
    OpData47 = 0x2f,
    OpData48 = 0x30,
    OpData49 = 0x31,
    OpData50 = 0x32,
    OpData51 = 0x33,
    OpData52 = 0x34,
    OpData53 = 0x35,
    OpData54 = 0x36,
    OpData55 = 0x37,
    OpData56 = 0x38,
    OpData57 = 0x39,
    OpData58 = 0x3a,
    OpData59 = 0x3b,
    OpData60 = 0x3c,
    OpData61 = 0x3d,
    OpData62 = 0x3e,
    OpData63 = 0x3f,
    OpData64 = 0x40,
    OpData65 = 0x41,
    OpData66 = 0x42,
    OpData67 = 0x43,
    OpData68 = 0x44,
    OpData69 = 0x45,
    OpData70 = 0x46,
    OpData71 = 0x47,
    OpData72 = 0x48,
    OpData73 = 0x49,
    OpData74 = 0x4a,
    OpData75 = 0x4b,
    OpPushData1 = 0x4c,
    OpPushData2 = 0x4d,
    OpPushData4 = 0x4e,
    Op1Negate = 0x4f,
    /**
     * Reserved
     */
    OpReserved = 0x50,
    Op1 = 0x51,
    Op2 = 0x52,
    Op3 = 0x53,
    Op4 = 0x54,
    Op5 = 0x55,
    Op6 = 0x56,
    Op7 = 0x57,
    Op8 = 0x58,
    Op9 = 0x59,
    Op10 = 0x5a,
    Op11 = 0x5b,
    Op12 = 0x5c,
    Op13 = 0x5d,
    Op14 = 0x5e,
    Op15 = 0x5f,
    Op16 = 0x60,
    OpNop = 0x61,
    /**
     * Reserved
     */
    OpVer = 0x62,
    OpIf = 0x63,
    OpNotIf = 0x64,
    /**
     * Reserved
     */
    OpVerIf = 0x65,
    /**
     * Reserved
     */
    OpVerNotIf = 0x66,
    OpElse = 0x67,
    OpEndIf = 0x68,
    OpVerify = 0x69,
    OpReturn = 0x6a,
    OpToAltStack = 0x6b,
    OpFromAltStack = 0x6c,
    Op2Drop = 0x6d,
    Op2Dup = 0x6e,
    Op3Dup = 0x6f,
    Op2Over = 0x70,
    Op2Rot = 0x71,
    Op2Swap = 0x72,
    OpIfDup = 0x73,
    OpDepth = 0x74,
    OpDrop = 0x75,
    OpDup = 0x76,
    OpNip = 0x77,
    OpOver = 0x78,
    OpPick = 0x79,
    OpRoll = 0x7a,
    OpRot = 0x7b,
    OpSwap = 0x7c,
    OpTuck = 0x7d,
    /**
     * Disabled
     */
    OpCat = 0x7e,
    /**
     * Disabled
     */
    OpSubStr = 0x7f,
    /**
     * Disabled
     */
    OpLeft = 0x80,
    /**
     * Disabled
     */
    OpRight = 0x81,
    OpSize = 0x82,
    /**
     * Disabled
     */
    OpInvert = 0x83,
    /**
     * Disabled
     */
    OpAnd = 0x84,
    /**
     * Disabled
     */
    OpOr = 0x85,
    /**
     * Disabled
     */
    OpXor = 0x86,
    OpEqual = 0x87,
    OpEqualVerify = 0x88,
    OpReserved1 = 0x89,
    OpReserved2 = 0x8a,
    Op1Add = 0x8b,
    Op1Sub = 0x8c,
    /**
     * Disabled
     */
    Op2Mul = 0x8d,
    /**
     * Disabled
     */
    Op2Div = 0x8e,
    OpNegate = 0x8f,
    OpAbs = 0x90,
    OpNot = 0x91,
    Op0NotEqual = 0x92,
    OpAdd = 0x93,
    OpSub = 0x94,
    /**
     * Disabled
     */
    OpMul = 0x95,
    /**
     * Disabled
     */
    OpDiv = 0x96,
    /**
     * Disabled
     */
    OpMod = 0x97,
    /**
     * Disabled
     */
    OpLShift = 0x98,
    /**
     * Disabled
     */
    OpRShift = 0x99,
    OpBoolAnd = 0x9a,
    OpBoolOr = 0x9b,
    OpNumEqual = 0x9c,
    OpNumEqualVerify = 0x9d,
    OpNumNotEqual = 0x9e,
    OpLessThan = 0x9f,
    OpGreaterThan = 0xa0,
    OpLessThanOrEqual = 0xa1,
    OpGreaterThanOrEqual = 0xa2,
    OpMin = 0xa3,
    OpMax = 0xa4,
    OpWithin = 0xa5,
    OpUnknown166 = 0xa6,
    OpUnknown167 = 0xa7,
    OpSha256 = 0xa8,
    OpCheckMultiSigECDSA = 0xa9,
    OpBlake2b = 0xaa,
    OpCheckSigECDSA = 0xab,
    OpCheckSig = 0xac,
    OpCheckSigVerify = 0xad,
    OpCheckMultiSig = 0xae,
    OpCheckMultiSigVerify = 0xaf,
    OpCheckLockTimeVerify = 0xb0,
    OpCheckSequenceVerify = 0xb1,
    OpUnknown178 = 0xb2,
    OpUnknown179 = 0xb3,
    OpUnknown180 = 0xb4,
    OpUnknown181 = 0xb5,
    OpUnknown182 = 0xb6,
    OpUnknown183 = 0xb7,
    OpUnknown184 = 0xb8,
    OpUnknown185 = 0xb9,
    OpUnknown186 = 0xba,
    OpUnknown187 = 0xbb,
    OpUnknown188 = 0xbc,
    OpUnknown189 = 0xbd,
    OpUnknown190 = 0xbe,
    OpUnknown191 = 0xbf,
    OpUnknown192 = 0xc0,
    OpUnknown193 = 0xc1,
    OpUnknown194 = 0xc2,
    OpUnknown195 = 0xc3,
    OpUnknown196 = 0xc4,
    OpUnknown197 = 0xc5,
    OpUnknown198 = 0xc6,
    OpUnknown199 = 0xc7,
    OpUnknown200 = 0xc8,
    OpUnknown201 = 0xc9,
    OpUnknown202 = 0xca,
    OpUnknown203 = 0xcb,
    OpUnknown204 = 0xcc,
    OpUnknown205 = 0xcd,
    OpUnknown206 = 0xce,
    OpUnknown207 = 0xcf,
    OpUnknown208 = 0xd0,
    OpUnknown209 = 0xd1,
    OpUnknown210 = 0xd2,
    OpUnknown211 = 0xd3,
    OpUnknown212 = 0xd4,
    OpUnknown213 = 0xd5,
    OpUnknown214 = 0xd6,
    OpUnknown215 = 0xd7,
    OpUnknown216 = 0xd8,
    OpUnknown217 = 0xd9,
    OpUnknown218 = 0xda,
    OpUnknown219 = 0xdb,
    OpUnknown220 = 0xdc,
    OpUnknown221 = 0xdd,
    OpUnknown222 = 0xde,
    OpUnknown223 = 0xdf,
    OpUnknown224 = 0xe0,
    OpUnknown225 = 0xe1,
    OpUnknown226 = 0xe2,
    OpUnknown227 = 0xe3,
    OpUnknown228 = 0xe4,
    OpUnknown229 = 0xe5,
    OpUnknown230 = 0xe6,
    OpUnknown231 = 0xe7,
    OpUnknown232 = 0xe8,
    OpUnknown233 = 0xe9,
    OpUnknown234 = 0xea,
    OpUnknown235 = 0xeb,
    OpUnknown236 = 0xec,
    OpUnknown237 = 0xed,
    OpUnknown238 = 0xee,
    OpUnknown239 = 0xef,
    OpUnknown240 = 0xf0,
    OpUnknown241 = 0xf1,
    OpUnknown242 = 0xf2,
    OpUnknown243 = 0xf3,
    OpUnknown244 = 0xf4,
    OpUnknown245 = 0xf5,
    OpUnknown246 = 0xf6,
    OpUnknown247 = 0xf7,
    OpUnknown248 = 0xf8,
    OpUnknown249 = 0xf9,
    OpSmallInteger = 0xfa,
    OpPubKeys = 0xfb,
    OpUnknown252 = 0xfc,
    OpPubKeyHash = 0xfd,
    OpPubKey = 0xfe,
    OpInvalidOpCode = 0xff,
}

"#;

///
///  ScriptBuilder provides a facility for building custom scripts. It allows
/// you to push opcodes, ints, and data while respecting canonical encoding. In
/// general it does not ensure the script will execute correctly, however any
/// data pushes which would exceed the maximum allowed script engine limits and
/// are therefore guaranteed not to execute will not be pushed and will result in
/// the Script function returning an error.
///
/// @see {@link Opcode}
/// @category Consensus
#[derive(Clone)]
#[wasm_bindgen(inspectable)]
pub struct ScriptBuilder {
    script_builder: Rc<RefCell<native::ScriptBuilder>>,
}

impl ScriptBuilder {
    #[inline]
    pub fn inner(&self) -> Ref<'_, native::ScriptBuilder> {
        self.script_builder.borrow()
    }

    #[inline]
    pub fn inner_mut(&self) -> RefMut<'_, native::ScriptBuilder> {
        self.script_builder.borrow_mut()
    }
}

impl Default for ScriptBuilder {
    fn default() -> Self {
        Self { script_builder: Rc::new(RefCell::new(kaspa_txscript::script_builder::ScriptBuilder::new())) }
    }
}

#[wasm_bindgen]
impl ScriptBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self::default()
    }

    #[wasm_bindgen(getter)]
    pub fn data(&self) -> HexString {
        self.script()
    }

    /// Get script bytes represented by a hex string.
    pub fn script(&self) -> HexString {
        let inner = self.inner();
        HexString::from(inner.script())
    }

    /// Drains (empties) the script builder, returning the
    /// script bytes represented by a hex string.
    pub fn drain(&self) -> HexString {
        let mut inner = self.inner_mut();
        HexString::from(inner.drain().as_slice())
    }

    #[wasm_bindgen(js_name = canonicalDataSize)]
    pub fn canonical_data_size(data: BinaryT) -> Result<u32> {
        let data = data.try_as_vec_u8()?;
        let size = native::ScriptBuilder::canonical_data_size(&data) as u32;
        Ok(size)
    }

    /// Pushes the passed opcode to the end of the script. The script will not
    /// be modified if pushing the opcode would cause the script to exceed the
    /// maximum allowed script engine size.
    #[wasm_bindgen(js_name = addOp)]
    pub fn add_op(&self, op: u8) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_op(op)?;
        Ok(self.clone())
    }

    /// Adds the passed opcodes to the end of the script.
    /// Supplied opcodes can be represented as a `Uint8Array` or a `HexString`.
    #[wasm_bindgen(js_name = "addOps")]
    pub fn add_ops(&self, opcodes: JsValue) -> Result<ScriptBuilder> {
        let opcodes = opcodes.try_as_vec_u8()?;
        self.inner_mut().add_ops(&opcodes)?;
        Ok(self.clone())
    }

    /// AddData pushes the passed data to the end of the script. It automatically
    /// chooses canonical opcodes depending on the length of the data.
    ///
    /// A zero length buffer will lead to a push of empty data onto the stack (Op0 = OpFalse)
    /// and any push of data greater than [`MAX_SCRIPT_ELEMENT_SIZE`](kaspa_txscript::MAX_SCRIPT_ELEMENT_SIZE) will not modify
    /// the script since that is not allowed by the script engine.
    ///
    /// Also, the script will not be modified if pushing the data would cause the script to
    /// exceed the maximum allowed script engine size [`MAX_SCRIPTS_SIZE`](kaspa_txscript::MAX_SCRIPTS_SIZE).
    #[wasm_bindgen(js_name = addData)]
    pub fn add_data(&self, data: BinaryT) -> Result<ScriptBuilder> {
        let data = data.try_as_vec_u8()?;

        let mut inner = self.inner_mut();
        inner.add_data(&data)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = addI64)]
    pub fn add_i64(&self, value: i64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_i64(value)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = addLockTime)]
    pub fn add_lock_time(&self, lock_time: u64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_lock_time(lock_time)?;
        Ok(self.clone())
    }

    #[wasm_bindgen(js_name = addSequence)]
    pub fn add_sequence(&self, sequence: u64) -> Result<ScriptBuilder> {
        let mut inner = self.inner_mut();
        inner.add_sequence(sequence)?;
        Ok(self.clone())
    }
}
