//!
//! Hex module provides a way to display binary data in a human-readable format.
//!

use hexplay::{
    color::{Color, Spec},
    HexView, HexViewBuilder,
};
use std::ops::Range;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

type Result<T> = std::result::Result<T, JsValue>;

#[derive(Default)]
pub struct HexViewConfig {
    pub offset: Option<usize>,
    pub replace_char: Option<char>,
    pub width: Option<usize>,
    pub colors: Option<Vec<(Spec, Range<usize>)>>,
}

impl HexViewConfig {
    pub fn build(self, slice: &[u8]) -> HexView<'_> {
        let mut builder = HexViewBuilder::new(slice);

        if let Some(offset) = self.offset {
            builder = builder.address_offset(offset);
        }

        if let Some(replace_char) = self.replace_char {
            builder = builder.replacement_character(replace_char);
        }

        if let Some(width) = self.width {
            builder = builder.row_width(width);
        }

        if let Some(colors) = self.colors {
            if !colors.is_empty() {
                builder = builder.add_colors(colors);
            }
        }

        builder.finish()
    }
}

pub struct ColorRange {
    pub color: Option<Color>,
    pub background: Option<Color>,
    pub range: Range<usize>,
}

impl ColorRange {
    fn new(color: Option<Color>, background: Option<Color>, range: Range<usize>) -> Self {
        Self { color, background, range }
    }

    fn into_tuple(self) -> (Spec, Range<usize>) {
        let mut spec = Spec::new();
        spec.set_fg(self.color);
        spec.set_bg(self.background);

        (spec, self.range)
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_HEX_VIEW: &'static str = r#"
/**
 * Color range configuration for Hex View.
 * 
 * @category General
 */ 
export interface IHexViewColor {
    start: number;
    end: number;
    color?: string;
    background?: string;
}

/**
 * Configuration interface for Hex View.
 * 
 * @category General
 */ 
export interface IHexViewConfig {
    offset? : number;
    replacementCharacter? : string;
    width? : number;
    colors? : IHexViewColor[];
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "IHexViewColor")]
    pub type HexViewColorT;
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "IHexViewColor[]")]
    pub type HexViewColorArrayT;
    #[wasm_bindgen(typescript_type = "IHexViewConfig")]
    pub type HexViewConfigT;
}

impl TryFrom<JsValue> for ColorRange {
    type Error = JsValue;
    fn try_from(js_value: JsValue) -> Result<ColorRange> {
        if let Some(object) = js_sys::Object::try_from(&js_value) {
            let start = object.get_u32("start")? as usize;
            let end = object.get_u32("end")? as usize;

            let color = object.get_string("color").ok();
            let color =
                color.map(|color| Color::from_str(color.as_str()).map_err(|e| JsValue::from_str(&e.to_string()))).transpose()?;

            let background = object.get_string("background").ok();
            let background = background
                .map(|background| Color::from_str(background.as_str()).map_err(|e| JsValue::from_str(&e.to_string())))
                .transpose()?;

            Ok(ColorRange::new(color, background, start..end))
        } else {
            Err(JsValue::from_str("color range must be an object"))
        }
    }
}

pub fn try_to_color_vec(js_value: JsValue) -> Result<Vec<(Spec, Range<usize>)>> {
    if js_value.is_array() {
        let list = js_sys::Array::from(&js_value).iter().map(TryFrom::try_from).collect::<Result<Vec<ColorRange>>>()?;
        Ok(list.into_iter().map(ColorRange::into_tuple).collect::<Vec<_>>())
    } else {
        let tuple = ColorRange::try_from(js_value).map(ColorRange::into_tuple)?;
        Ok(vec![tuple])
    }
}

impl TryFrom<HexViewConfigT> for HexViewConfig {
    type Error = JsValue;
    fn try_from(js_value: HexViewConfigT) -> Result<HexViewConfig> {
        let object = js_sys::Object::try_from(&js_value).ok_or_else(|| JsValue::from_str("HexView config must be an object"))?;

        let offset = object.get_u32("offset").ok().map(|v| v as usize);
        let replace_char = object.get_string("replacementCharacter").ok().map(|s| s.chars().next().unwrap_or(' '));
        let width = object.get_u32("width").ok().map(|v| v as usize);
        let colors = object.get_value("colors").ok().map(try_to_color_vec).transpose()?;

        Ok(HexViewConfig { offset, replace_char, width, colors })
    }
}
