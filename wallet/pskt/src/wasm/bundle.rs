use super::error::*;
use super::result::*;
use crate::bundle::Bundle;
use crate::pskt::Inner;
use wasm_bindgen::prelude::*;
use crate::wasm::pskt::PSKT;

#[wasm_bindgen(getter_with_clone)]
#[derive(Debug)]
pub struct PsktBundle(Bundle);

impl Clone for Bundle {
    fn clone(&self) -> Self {
        Bundle(self.0.clone())
    }
}

#[wasm_bindgen]
impl PsktBundle {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PsktBundle> {
        let bundle = Bundle::new();
        Ok(PsktBundle(bundle))
    }

    #[wasm_bindgen]
    pub fn serialize(&self) -> Result<String> {
        self.0.serialize().map_err(Error::from)
    }

    #[wasm_bindgen]
    pub fn deserialize(hex_data: &str) -> Result<PsktBundle> {
        let bundle = Bundle::deserialize(hex_data).map_err(Error::from)?;
        Ok(PsktBundle(bundle))
    }

    #[wasm_bindgen]
    pub fn pskt_count(&self) -> usize {
        self.0.0.len()
    }

    #[wasm_bindgen]
    pub fn add_pskt(&mut self, pskt: &PSKT) -> Result<()> {
        // Use the payload_getter to retrieve the inner data
        let payload = pskt.payload_getter();
        let inner: Inner = serde_wasm_bindgen::from_value(payload)
            .map_err(|_| Error::Custom("Failed to convert PSKT payload".to_string()))?;
        
        self.0.add_inner(inner);
        Ok(())
    }

    #[wasm_bindgen]
    pub fn merge(&mut self, other: &PsktBundle) {
        self.0.merge(other.0.clone());
    }
}

impl From<Bundle> for PsktBundle {
    fn from(bundle: Bundle) -> Self {
        PsktBundle(bundle)
    }
}

impl From<PsktBundle> for Bundle {
    fn from(bundle: PsktBundle) -> Self {
        bundle.0
    }
}

impl AsRef<Bundle> for PsktBundle {
    fn as_ref(&self) -> &Bundle {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::wasm;

    use super::*;
    use wasm_bindgen_test::*;
    use wasm_bindgen::convert::IntoWasmAbi;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn test_pskt_bundle_creation() {
        let bundle = PsktBundle::new().expect("Failed to create PsktBundle");
        assert_eq!(bundle.pskt_count(), 0, "New bundle should have zero PSKTs");
    }

    #[wasm_bindgen_test]
    fn test_pskt_bundle_serialize_deserialize() {
        // Create a bundle
        let mut bundle = PsktBundle::new().expect("Failed to create PsktBundle");
        
        // Serialize the bundle
        let serialized = bundle.serialize().expect("Failed to serialize bundle");
        
        // Deserialize the bundle
        let deserialized_bundle = PsktBundle::deserialize(&serialized)
            .expect("Failed to deserialize bundle");
        
        // Check that the deserialized bundle has the same number of PSKTs
        assert_eq!(
            bundle.pskt_count(), 
            deserialized_bundle.pskt_count(), 
            "Deserialized bundle should have same PSKT count"
        );
    }

}