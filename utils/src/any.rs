/// Provides a best-effort short type name
pub fn type_name_short<T: ?Sized>() -> &'static str {
    let s = std::any::type_name::<T>();
    const SEP: &str = "::";
    if s.find('<').is_some() {
        // T is generic so would require more sophisticated processing
        return s;
    }
    match s.rfind(SEP) {
        Some(i) => s.split_at(i + SEP.len()).1,
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn test_type_name_short() {
        assert_eq!("u64", type_name_short::<u64>());
        assert_eq!("IpAddr", type_name_short::<IpAddr>());
        assert_eq!(
            std::any::type_name::<Option<String>>(),
            type_name_short::<Option<String>>(),
            "generic types are expected to remain long"
        );
    }
}
