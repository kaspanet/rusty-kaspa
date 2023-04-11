use wasm_bindgen::prelude::*;

#[macro_export]
macro_rules! construct_xuint {
    ($name:ident) => {
        #[wasm_bindgen(inspectable)]
        pub struct $name {
            inner: $crate::$name,
        }

        #[wasm_bindgen]
        impl $name {
            #[wasm_bindgen(constructor)]
            pub fn new(n: u64) -> $name {
                Self { inner: $crate::$name::from_u64(n) }
            }

            #[wasm_bindgen(js_name=fromHex)]
            pub fn from_hex(hex: &str) -> Result<$name, JsError> {
                Ok(Self { inner: $crate::$name::from_hex(hex)? })
            }

            #[wasm_bindgen(js_name=toBigInt)]
            pub fn to_bigint(&self) -> Result<js_sys::BigInt, JsError> {
                let value = js_sys::BigInt::new(&JsValue::from(self.inner.as_u128())).map_err(|err| {
                    let err: String = err.to_string().into();
                    JsError::new(err.as_str())
                })?;
                Ok(value)
            }

            // #[wasm_bindgen(js_name=toJSON)]
            // pub fn to_json(&self) -> Result<bool, JsError> {
            //     let obj = js_sys::Object::new();
            //     //obj.
            //     //self.to_bigint()
            //     Ok(true)
            // }

            #[wasm_bindgen(getter)]
            pub fn value(&self) -> Result<js_sys::BigInt, JsError> {
                self.to_bigint()
            }
        }

        impl From<$name> for $crate::$name {
            fn from(v: $name) -> Self {
                v.inner
            }
        }

        impl From<$crate::$name> for $name {
            fn from(inner: $crate::$name) -> Self {
                Self { inner }
            }
        }
    };
}

construct_xuint!(Uint192);
construct_xuint!(Uint256);
