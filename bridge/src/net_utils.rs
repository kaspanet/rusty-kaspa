/// Shared utilities for normalizing ports/bind addresses.
///
/// We intentionally keep the logic here minimal and dependency-free so both the library
/// (web/prom servers) and the binary (CLI/config parsing) can use the same behavior.
///
/// Supported input examples:
/// - ":3030"          -> ":3030"
/// - "3030"           -> ":3030"
/// - "127.0.0.1:3030" -> "127.0.0.1:3030"
/// - "0.0.0.0:3030"   -> "0.0.0.0:3030"
pub fn normalize_port(port_or_addr: &str) -> String {
    let s = port_or_addr.trim();
    if s.starts_with(':') {
        s.to_string()
    } else if s.chars().all(|c| c.is_ascii_digit()) {
        format!(":{}", s)
    } else {
        s.to_string()
    }
}

/// Convert a port-or-address string into a concrete bind address suitable for `SocketAddr::parse()`.
///
/// `":3030"` becomes `"0.0.0.0:3030"` for backward compatibility with existing config.
pub fn bind_addr_from_port(port_or_addr: &str) -> String {
    let s = normalize_port(port_or_addr);
    if s.starts_with(':') { format!("0.0.0.0{}", s) } else { s }
}
