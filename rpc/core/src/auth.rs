use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub enum AuthMode {
    #[default]
    Disabled,
    Unsafe {
        exclusions: HashSet<String>,
    },
    All,
}

impl AuthMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        let parts: Vec<&str> = value.split(',').collect();
        match parts[0].trim() {
            "all" => Ok(AuthMode::All),
            "unsafe" => {
                let exclusions = parts[1..].iter().map(|s| s.trim().trim_start_matches('-').to_string()).collect();
                Ok(AuthMode::Unsafe { exclusions })
            }
            other => Err(format!("Invalid auth mode: '{other}'. Expected 'unsafe', 'all', or 'unsafe,-Method1,-Method2'")),
        }
    }
}

pub struct RpcAuthConfig {
    pub mode: AuthMode,
    expected_token: [u8; 32],
    pub cookie_path: PathBuf,
}

impl RpcAuthConfig {
    pub fn new(mode: AuthMode, secret: [u8; 32], cookie_path: PathBuf) -> Self {
        Self { mode, expected_token: secret, cookie_path }
    }

    pub fn verify_token(&self, provided_hex: &str) -> bool {
        if provided_hex.len() != 64 {
            return false;
        }
        let mut provided = [0u8; 32];
        if faster_hex::hex_decode(provided_hex.as_bytes(), &mut provided).is_err() {
            return false;
        }
        // Byte-by-byte comparison (timing attacks are not practical on localhost)
        self.expected_token == provided
    }

    pub fn requires_auth_for_unsafe(&self, method_name: &str) -> bool {
        match &self.mode {
            AuthMode::Disabled => false,
            AuthMode::All => true,
            AuthMode::Unsafe { exclusions } => !exclusions.contains(method_name),
        }
    }

    pub fn requires_auth_for_any(&self) -> bool {
        matches!(self.mode, AuthMode::All)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_unsafe() {
        let mode = AuthMode::parse("unsafe").unwrap();
        assert!(matches!(mode, AuthMode::Unsafe { exclusions } if exclusions.is_empty()));
    }

    #[test]
    fn test_parse_all() {
        let mode = AuthMode::parse("all").unwrap();
        assert!(matches!(mode, AuthMode::All));
    }

    #[test]
    fn test_parse_unsafe_with_exclusions() {
        let mode = AuthMode::parse("unsafe,-Ban,-Unban").unwrap();
        if let AuthMode::Unsafe { exclusions } = mode {
            assert!(exclusions.contains("Ban"));
            assert!(exclusions.contains("Unban"));
            assert!(!exclusions.contains("Shutdown"));
        } else {
            panic!("Expected Admin mode");
        }
    }

    #[test]
    fn test_parse_invalid() {
        assert!(AuthMode::parse("invalid").is_err());
    }

    #[test]
    fn test_verify_token() {
        let secret = [0xABu8; 32];
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions: HashSet::new() }, secret, PathBuf::new());
        let hex = faster_hex::hex_string(&secret);
        assert!(config.verify_token(&hex));
        assert!(!config.verify_token("0000000000000000000000000000000000000000000000000000000000000000"));
        assert!(!config.verify_token("short"));
    }

    #[test]
    fn test_requires_auth_for_unsafe() {
        let mut exclusions = HashSet::new();
        exclusions.insert("Ban".to_string());
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions }, [0u8; 32], PathBuf::new());
        assert!(config.requires_auth_for_unsafe("Shutdown"));
        assert!(!config.requires_auth_for_unsafe("Ban"));
    }

    #[test]
    fn test_requires_auth_disabled() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.requires_auth_for_unsafe("Shutdown"));
        assert!(!config.requires_auth_for_any());
    }

    #[test]
    fn test_requires_auth_all() {
        let config = RpcAuthConfig::new(AuthMode::All, [0u8; 32], PathBuf::new());
        assert!(config.requires_auth_for_unsafe("Shutdown"));
        assert!(config.requires_auth_for_any());
    }
}
