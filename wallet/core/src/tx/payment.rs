use crate::imports::*;
use kaspa_consensus_wasm::{TransactionOutput, TransactionOutputInner};
use kaspa_txscript::pay_to_address_script;

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

#[derive(Debug)]
#[wasm_bindgen(inspectable)]
pub struct PaymentOutput {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
}

impl TryFrom<JsValue> for PaymentOutput {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        if let Ok(array) = js_value.clone().dyn_into::<Array>() {
            let length = array.length();
            if length != 2 {
                Err(Error::Custom("Invalid payment output".to_string()))
            } else {
                let address = Address::try_from(array.get(0))?;
                let amount = array.get(1).try_as_u64()?;
                Ok(Self { address, amount })
            }
        } else if let Some(object) = Object::try_from(&js_value) {
            let address = object.get::<Address>("address")?;
            let amount = object.get_u64("amount")?;
            Ok(Self { address, amount })
        } else {
            Err(Error::Custom("Invalid payment output".to_string()))
        }
    }
}

#[wasm_bindgen]
impl PaymentOutput {
    #[wasm_bindgen(constructor)]
    pub fn new(address: Address, amount: u64) -> Self {
        Self { address, amount }
    }
}

impl From<PaymentOutput> for TransactionOutput {
    fn from(value: PaymentOutput) -> Self {
        Self::new_with_inner(TransactionOutputInner { script_public_key: pay_to_address_script(&value.address), value: value.amount })
    }
}

impl From<PaymentOutput> for PaymentDestination {
    fn from(output: PaymentOutput) -> Self {
        Self::PaymentOutputs(PaymentOutputs { outputs: vec![output] })
    }
}

#[derive(Debug)]
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
    pub fn constructor(output_array: JsValue) -> crate::Result<PaymentOutputs> {
        let mut outputs = vec![];
        let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
        for x in iterator {
            outputs.push((x?).try_into()?);
        }

        Ok(Self { outputs })
    }
}

impl TryFrom<JsValue> for PaymentOutputs {
    type Error = Error;
    fn try_from(js_value: JsValue) -> Result<Self, Self::Error> {
        let outputs = if let Some(output_array) = js_value.dyn_ref::<js_sys::Array>() {
            let vec = output_array.to_vec();
            vec.into_iter().map(PaymentOutput::try_from).collect::<Result<Vec<_>, _>>()?
        } else if let Some(object) = js_value.dyn_ref::<js_sys::Object>() {
            Object::entries(object).iter().map(PaymentOutput::try_from).collect::<Result<Vec<_>, _>>()?
        } else if let Some(map) = js_value.dyn_ref::<js_sys::Map>() {
            map.entries().into_iter().flat_map(|v| v.map(PaymentOutput::try_from)).collect::<Result<Vec<_>, _>>()?
        } else {
            return Err(Error::Custom("payment outputs must be an array or an object".to_string()));
        };

        Ok(Self { outputs })
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
