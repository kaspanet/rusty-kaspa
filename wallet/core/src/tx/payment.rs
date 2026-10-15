//!
//! Primitives for declaring transaction payment destinations.
//!

use crate::imports::*;
use kaspa_consensus_client::{CovenantBinding as ClientCovenantBinding, TransactionOutput, TransactionOutputInner};
use kaspa_txscript::pay_to_address_script;

#[wasm_bindgen(typescript_custom_section)]
const TS_PAYMENT_OUTPUTS: &'static str = r#"
/**
 *
 * Defines a single payment output.
 *
 * @see {@link IGeneratorSettingsObject}, {@link Generator}
 * @category Wallet SDK
 */
export interface IPaymentOutput {
    /**
     * Destination address. The address prefix must match the network
     * you are transacting on (e.g. `kaspa:` for mainnet, `kaspatest:` for testnet, etc).
     */
    address: Address | string;
    /**
     * Output amount in SOMPI.
     */
    amount: bigint;
    /**
     * Optional covenant binding for the output. Requires transaction version >= 1.
     * Warning: isn't supported by the transaction generator, instead use createTransaction()
     */
    covenant?: ICovenantBinding | CovenantBinding;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing a single payment output (`IPaymentOutput`).
    /// @category Wallet SDK
    #[wasm_bindgen(typescript_type = "IPaymentOutput")]
    pub type IPaymentOutput;
    /// WASM (TypeScript) type representing multiple payment outputs (`IPaymentOutput[]`).
    /// @category Wallet SDK
    #[wasm_bindgen(typescript_type = "IPaymentOutput[]")]
    pub type IPaymentOutputArray;
}

/// A Rust data structure representing a payment destination.
/// A payment destination is used to signal Generator where to send the funds.
/// The destination can be a change address or a set of [`PaymentOutput`].
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum PaymentDestination {
    Change,
    PaymentOutputs(PaymentOutputs),
}

impl PaymentDestination {
    pub fn amount(&self) -> Option<u64> {
        match self {
            Self::Change => None,
            Self::PaymentOutputs(payment_outputs) => Some(payment_outputs.amount()),
        }
    }
}

/// A Rust data structure representing a single payment
/// output containing a destination address, amount and covenant.
///
/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct PaymentOutput {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
    #[wasm_bindgen(getter_with_clone)]
    pub covenant: Option<ClientCovenantBinding>,
}

impl TryCastFromJs for PaymentOutput {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let address = object.cast_into::<Address>("address")?;
                let amount = object.get_u64("amount")?;
                let covenant = object
                    .try_get_value("covenant")?
                    .map(|v| v.try_into_owned().map_err(|err| kaspa_consensus_client::error::Error::convert("covenant", err)))
                    .transpose()?;
                Ok(Self { address, amount, covenant })
            } else {
                Err(Error::Custom("Invalid payment output".to_string()))
            }
        })
    }
}

#[wasm_bindgen]
impl PaymentOutput {
    /// Main constructor (no covenant)
    #[wasm_bindgen(constructor)]
    pub fn new(address: Address, amount: u64) -> Self {
        Self { address, amount, covenant: None }
    }

    /// Factory method for covenant variant
    #[wasm_bindgen(js_name = withCovenant)]
    pub fn with_covenant(address: Address, amount: u64, covenant: ClientCovenantBinding) -> Self {
        Self { address, amount, covenant: Some(covenant) }
    }
}

impl From<PaymentOutput> for TransactionOutput {
    fn from(value: PaymentOutput) -> Self {
        Self::new_with_inner(TransactionOutputInner {
            script_public_key: pay_to_address_script(&value.address),
            value: value.amount,
            covenant: value.covenant,
        })
    }
}

impl From<PaymentOutput> for PaymentDestination {
    fn from(output: PaymentOutput) -> Self {
        Self::PaymentOutputs(PaymentOutputs { outputs: vec![output] })
    }
}

/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[wasm_bindgen]
pub struct PaymentOutputs {
    #[wasm_bindgen(skip)]
    pub outputs: Vec<PaymentOutput>,
}

impl PaymentOutputs {
    pub fn amount(&self) -> u64 {
        self.outputs.iter().map(|payment_output| payment_output.amount).sum()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PaymentOutput> {
        self.outputs.iter()
    }
}

impl From<PaymentOutputs> for PaymentDestination {
    fn from(outputs: PaymentOutputs) -> Self {
        Self::PaymentOutputs(outputs)
    }
}

#[wasm_bindgen]
impl PaymentOutputs {
    #[wasm_bindgen(constructor)]
    pub fn constructor(output_array: IPaymentOutputArray) -> crate::result::Result<PaymentOutputs> {
        let mut outputs = vec![];
        let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
        for x in iterator {
            // outputs.push((x?).try_into_cast()?);
            outputs.push(PaymentOutput::try_owned_from(x?)?);
        }

        Ok(Self { outputs })
    }
}

impl TryCastFromJs for PaymentOutputs {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let outputs = if let Some(output_array) = value.as_ref().dyn_ref::<js_sys::Array>() {
                // it's an array, expected to be of PaymentOutput
                let vec = output_array.to_vec();
                vec.into_iter().map(PaymentOutput::try_owned_from).collect::<Result<Vec<_>, _>>()?
            } else if Object::try_from(value.as_ref()).is_some() {
                // it's an object, expected to be a PaymentOutput directly
                vec![PaymentOutput::try_owned_from(value)?]
            } else {
                return Err(Error::Custom("payment outputs must be an array or an object".to_string()));
            };

            Ok(Self { outputs })
        })
    }
}

impl From<PaymentOutputs> for Vec<TransactionOutput> {
    fn from(value: PaymentOutputs) -> Self {
        value.outputs.into_iter().map(TransactionOutput::from).collect()
    }
}

impl From<(Address, u64)> for PaymentOutputs {
    fn from((address, amount): (Address, u64)) -> Self {
        PaymentOutputs { outputs: vec![PaymentOutput::new(address, amount)] }
    }
}

impl From<(&Address, u64)> for PaymentOutputs {
    fn from((address, amount): (&Address, u64)) -> Self {
        PaymentOutputs { outputs: vec![PaymentOutput::new(address.clone(), amount)] }
    }
}

impl From<&[(Address, u64)]> for PaymentOutputs {
    fn from(outputs: &[(Address, u64)]) -> Self {
        let outputs = outputs.iter().map(|(address, amount)| PaymentOutput::new(address.clone(), *amount)).collect();
        PaymentOutputs { outputs }
    }
}

impl From<&[(&Address, u64)]> for PaymentOutputs {
    fn from(outputs: &[(&Address, u64)]) -> Self {
        let outputs = outputs.iter().map(|(address, amount)| PaymentOutput::new((*address).clone(), *amount)).collect();
        PaymentOutputs { outputs }
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn test_payment_output_try_cast_from_plain_object_with_covenant() {
        let address = Address::try_from("kaspatest:qqz22l98sf8jun72rwh5rqe2tm8lhwtdxdmynrz4ypwak427qed5juktjt7ju").unwrap();
        let amount = 1234;
        let covenant_id = Hash::from_bytes([0xab; 32]);

        let covenant = Object::new();
        covenant.set("authorizingInput", &JsValue::from(2)).unwrap();
        covenant.set("covenantId", &JsValue::from_str(&covenant_id.to_string())).unwrap();

        let output = Object::new();
        output.set("address", &JsValue::from_str(&address.to_string())).unwrap();
        output.set("amount", &JsValue::from(amount)).unwrap();
        output.set("covenant", &covenant.into()).unwrap();

        let output = PaymentOutput::try_owned_from(output).expect("try_cast_from should accept an IPaymentOutput object");
        let covenant = output.covenant.expect("covenant should be present");

        assert_eq!(output.address, address);
        assert_eq!(output.amount, amount);
        assert_eq!(covenant.get_authorizing_input(), 2);
        assert_eq!(covenant.get_covenant_id(), covenant_id);
    }

    #[wasm_bindgen_test]
    fn test_payment_outputs_try_cast_from_single_and_array_objects() {
        let address = Address::try_from("kaspatest:qqz22l98sf8jun72rwh5rqe2tm8lhwtdxdmynrz4ypwak427qed5juktjt7ju").unwrap();

        let output = Object::new();
        output.set("address", &JsValue::from_str(&address.to_string())).unwrap();
        output.set("amount", &JsValue::from(1000)).unwrap();

        let single = PaymentOutputs::try_owned_from(output.clone()).expect("try_cast_from should accept one IPaymentOutput object");
        assert_eq!(single.outputs.len(), 1);
        assert_eq!(single.outputs[0].address, address);
        assert_eq!(single.outputs[0].amount, 1000);

        let output_2 = Object::new();
        output_2.set("address", &JsValue::from_str(&address.to_string())).unwrap();
        output_2.set("amount", &JsValue::from(2000)).unwrap();

        let outputs = Array::new();
        outputs.push(&output.into());
        outputs.push(&output_2.into());

        let outputs = PaymentOutputs::try_owned_from(outputs).expect("try_cast_from should accept an IPaymentOutput array");
        assert_eq!(outputs.outputs.len(), 2);
        assert_eq!(outputs.outputs[0].amount, 1000);
        assert_eq!(outputs.outputs[1].amount, 2000);
    }
}
