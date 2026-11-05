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

    // =========================================================================
    // A. AuthMode parsing
    // =========================================================================

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
            panic!("Expected Unsafe mode");
        }
    }

    #[test]
    fn test_parse_invalid() {
        assert!(AuthMode::parse("invalid").is_err());
    }

    #[test]
    fn test_parse_empty_string() {
        assert!(AuthMode::parse("").is_err());
    }

    #[test]
    fn test_parse_unsafe_no_dash_prefix() {
        // "unsafe,Ban" (without dash) should still work — trim_start_matches('-')
        let mode = AuthMode::parse("unsafe,Ban").unwrap();
        if let AuthMode::Unsafe { exclusions } = mode {
            assert!(exclusions.contains("Ban"));
        } else {
            panic!("Expected Unsafe mode");
        }
    }

    // =========================================================================
    // B. Token verification
    // =========================================================================

    #[test]
    fn test_verify_token_valid() {
        let secret = [0xABu8; 32];
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions: HashSet::new() }, secret, PathBuf::new());
        let hex = faster_hex::hex_string(&secret);
        assert!(config.verify_token(&hex));
    }

    #[test]
    fn test_verify_token_wrong_secret() {
        let secret = [0xABu8; 32];
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions: HashSet::new() }, secret, PathBuf::new());
        assert!(!config.verify_token("0000000000000000000000000000000000000000000000000000000000000000"));
    }

    #[test]
    fn test_verify_token_too_short() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.verify_token("short"));
    }

    #[test]
    fn test_verify_token_too_long() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.verify_token(&"ab".repeat(33))); // 66 chars
    }

    #[test]
    fn test_verify_token_invalid_hex() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.verify_token(&"zz".repeat(32))); // 64 chars but not hex
    }

    #[test]
    fn test_verify_token_case_sensitive() {
        let secret = [0xABu8; 32];
        let config = RpcAuthConfig::new(AuthMode::Disabled, secret, PathBuf::new());
        let hex_lower = faster_hex::hex_string(&secret); // lowercase
        assert!(config.verify_token(&hex_lower));
        // uppercase should also decode to same bytes
        assert!(config.verify_token(&hex_lower.to_uppercase()));
    }

    #[test]
    fn test_verify_token_empty() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.verify_token(""));
    }

    // =========================================================================
    // C. Auth requirement logic — requires_auth_for_unsafe
    // =========================================================================

    #[test]
    fn test_disabled_never_requires_auth() {
        let config = RpcAuthConfig::new(AuthMode::Disabled, [0u8; 32], PathBuf::new());
        assert!(!config.requires_auth_for_unsafe("Shutdown"));
        assert!(!config.requires_auth_for_unsafe("Ban"));
        assert!(!config.requires_auth_for_unsafe("GetInfo"));
        assert!(!config.requires_auth_for_any());
    }

    #[test]
    fn test_all_requires_auth_for_everything() {
        let config = RpcAuthConfig::new(AuthMode::All, [0u8; 32], PathBuf::new());
        assert!(config.requires_auth_for_unsafe("Shutdown"));
        assert!(config.requires_auth_for_unsafe("GetInfo"));
        assert!(config.requires_auth_for_any());
    }

    #[test]
    fn test_unsafe_mode_requires_auth_by_default() {
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions: HashSet::new() }, [0u8; 32], PathBuf::new());
        assert!(config.requires_auth_for_unsafe("Shutdown"));
        assert!(config.requires_auth_for_unsafe("Ban"));
        assert!(config.requires_auth_for_unsafe("AddPeer"));
        // Unsafe mode does NOT require auth for "any" (public methods)
        assert!(!config.requires_auth_for_any());
    }

    #[test]
    fn test_unsafe_mode_exclusions_skip_auth() {
        let mut exclusions = HashSet::new();
        exclusions.insert("Ban".to_string());
        exclusions.insert("Unban".to_string());
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions }, [0u8; 32], PathBuf::new());
        // Excluded methods: no auth required
        assert!(!config.requires_auth_for_unsafe("Ban"));
        assert!(!config.requires_auth_for_unsafe("Unban"));
        // Non-excluded: auth required
        assert!(config.requires_auth_for_unsafe("Shutdown"));
        assert!(config.requires_auth_for_unsafe("AddPeer"));
        assert!(config.requires_auth_for_unsafe("ResolveFinalityConflict"));
    }

    // =========================================================================
    // D. Safety: new methods default to auth-required (fail-safe)
    // =========================================================================

    #[test]
    fn test_new_method_defaults_to_auth_required() {
        // Simulates adding a new unsafe method in the future.
        // Without adding it to the exclusion list, auth should be required.
        let mut exclusions = HashSet::new();
        exclusions.insert("Ban".to_string());
        let config = RpcAuthConfig::new(AuthMode::Unsafe { exclusions }, [0u8; 32], PathBuf::new());
        assert!(config.requires_auth_for_unsafe("FutureNewDangerousMethod"));
    }

    // =========================================================================
    // E. Backward compatibility: no auth_config = zero overhead
    // =========================================================================

    #[test]
    fn test_no_auth_config_none_check() {
        // Simulates the runtime check: auth_config is Option<Arc<RpcAuthConfig>>.
        // When None, no auth checks run at all.
        let auth_config: Option<RpcAuthConfig> = None;
        // This pattern mirrors what the macros and require_unsafe/require_any_auth do:
        let needs_auth = auth_config.as_ref().map_or(false, |a| a.requires_auth_for_any());
        assert!(!needs_auth);
        let needs_unsafe_auth = auth_config.as_ref().map_or(false, |a| a.requires_auth_for_unsafe("Shutdown"));
        assert!(!needs_unsafe_auth);
    }
}
