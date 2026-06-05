use wasm_bindgen::prelude::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_BLOCK_COLOR: &'static str = r#"
/**
 * Block Color
 *
 * @category Consensus
 */
 export type BlockColor = "blue" | "red" | "unknown";
 "#;
