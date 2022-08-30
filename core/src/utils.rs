#[macro_export]
macro_rules! extract_enum_value {
    ($value:expr, $pattern:pat => $extracted_value:expr) => {
        match $value {
            $pattern => $extracted_value,
            _ => panic!("Pattern doesn't match!"),
        }
    };
}
