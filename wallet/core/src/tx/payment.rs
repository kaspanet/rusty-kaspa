use crate::imports::*;
use crate::wasm::tx::{TransactionOutput, TransactionOutputInner};
use kaspa_txscript::pay_to_address_script;
// use workflow_core::traits::IsNotEmpty;

pub enum PaymentDestination {
    // Address(Address),
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

    // pub fn mass(&self) -> u64 {

    // }
}

// impl From<Address> for PaymentDestination {
//     fn from(address: Address) -> Self {
//         Self::Address(address)
//     }
// }

#[derive(Debug)]
// #[wasm_bindgen(inspectable)]
// #[allow(dead_code)] //TODO: remove me
pub struct PaymentOutput {
    // #[wasm_bindgen(getter_with_clone)]
    pub address: Address,
    pub amount: u64,
    // utxo_entry: Option<Arc<UtxoEntry>>,
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
            let address = Address::try_from(object.get("address")?)?; //.ok().map(Address::try_from)?;
            let amount = object.get_u64("amount")?;
            Ok(Self { address, amount })
        } else {
            Err(Error::Custom("Invalid payment output".to_string()))
        }
    }
}

// #[wasm_bindgen]
impl PaymentOutput {
    // #[wasm_bindgen(constructor)]
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
// #[wasm_bindgen]
pub struct PaymentOutputs {
    // #[wasm_bindgen(skip)]
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

// #[wasm_bindgen]
// impl PaymentOutputs {
//     #[wasm_bindgen(constructor)]
//     pub fn constructor(output_array: JsValue) -> crate::Result<PaymentOutputs> {
//         let mut outputs = vec![];
//         let iterator = js_sys::try_iter(&output_array)?.ok_or("need to pass iterable JS values!")?;
//         for x in iterator {
//             outputs.push(from_value(x?)?);
//         }

//         Ok(Self { outputs })
//     }
// }

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

impl TryFrom<(Address, u64)> for PaymentOutputs {
    type Error = Error;
    fn try_from((address, amount): (Address, u64)) -> Result<Self, Self::Error> {
        Ok(PaymentOutputs { outputs: vec![PaymentOutput::new(address, amount)] })
    }
}
