use std::fmt::Display;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::num::ParseIntError;

#[derive(Debug, PartialEq, Eq)]
pub struct ParseHostOutput<'a> {
    pub scheme: Option<&'a str>,
    pub host: Host<'a>,
    pub port: Option<u16>,
    pub path: &'a str,
}

impl Display for ParseHostOutput<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(scheme) = self.scheme {
            write!(f, "{}://", scheme)?;
        }
        write!(f, "{}", self.host)?;
        if let Some(port) = self.port {
            write!(f, ":{}", port)?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Host<'a> {
    Domain(&'a str),
    Hostname(&'a str),
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
}

impl Display for Host<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Host::Domain(domain) => write!(f, "{}", domain),
            Host::Hostname(hostname) => write!(f, "{}", hostname),
            Host::Ipv4(ipv4) => write!(f, "{}", ipv4),
            Host::Ipv6(ipv6) => write!(f, "[{}]", ipv6),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseHostError {
    #[error("Invalid input")]
    InvalidInput,
    #[error("Invalid port: {0}")]
    ParsePortError(ParseIntError),
}

/// Parses a host string into a scheme, host, and port.
///
/// The host string can either be a hostname, domain, IPv4 address, or IPv6 address with optionally including the scheme and port.
///
/// IPv6 addresses are optionally enclosed in square brackets, and required if specifying a port.
///
/// If a path is attached to the host string, it will not be discarded.
pub fn parse_host(input: &str) -> Result<ParseHostOutput, ParseHostError> {
    // Attempt to split the input into scheme, host, and port.
    let (scheme, input) = match input.find("://") {
        Some(pos) => {
            let (scheme, input) = input.split_at(pos);
            (Some(scheme), &input[3..])
        }
        None => (None, input),
    };
    // Attempt to split path and host.
    let (input, path) = match input.find('/') {
        Some(pos) => input.split_at(pos),
        None => (input, ""),
    };

    let (host, port_str) = match input.rfind(':') {
        Some(pos) => {
            // Check if char before ':' is also ':'.
            // As that would mean that the ':' is part of an IPv6 address.
            // Needs to be checked in case ':' is the first character in the input.
            let Some(prev_pos) = pos.checked_sub(1) else {
                return Err(ParseHostError::InvalidInput);
            };
            if input.chars().nth(prev_pos) == Some(':') {
                (input, None)
            } else {
                let (host, port_str) = input.split_at(pos);
                (host, Some(&port_str[1..]))
            }
        }
        None => (input, None),
    };

    if host.is_empty() {
        return Err(ParseHostError::InvalidInput);
    }

    let port = port_str
        .map(|port_str| match port_str.parse::<u16>() {
            Ok(port) => Ok(port),
            Err(err) => Err(ParseHostError::ParsePortError(err)),
        })
        .map_or(Ok(None), |port| port.map(Some))?;

    // Attempt to parse the host as an IPv4 address.
    if let Ok(ipv4) = host.parse::<Ipv4Addr>() {
        return Ok(ParseHostOutput { scheme, host: Host::Ipv4(ipv4), port, path });
    }

    // Attempt to parse the host as an IPv6 address enclosed in square brackets.
    if host.starts_with('[') && host.ends_with(']') {
        let ipv6 = &host[1..host.len() - 1];
        if let Ok(ipv6) = ipv6.parse::<Ipv6Addr>() {
            return Ok(ParseHostOutput { scheme, host: Host::Ipv6(ipv6), port, path });
        }
    }
    // Attempt to parse the host as an IPv6 address.
    if let Ok(ipv6) = host.parse::<Ipv6Addr>() {
        return Ok(ParseHostOutput { scheme, host: Host::Ipv6(ipv6), port, path });
    }

    // Attempt to parse the host as a hostname.
    if host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Ok(ParseHostOutput { scheme, host: Host::Hostname(host), port, path });
    }

    // Attempt to parse the host as a domain.
    let does_not_start_with_dot = !host.starts_with('.');
    let does_not_end_with_dot = !host.ends_with('.');
    let has_at_least_one_dot = host.contains('.');
    let dots_are_separated_by_valid_chars = host.split('.').all(|part| {
        let part_does_not_start_with_hyphen = !part.starts_with('-');
        let part_does_not_end_with_hyphen = !part.ends_with('-');
        part_does_not_start_with_hyphen && part_does_not_end_with_hyphen && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    let does_not_start_with_hyphen = !host.starts_with('-');
    let does_not_end_with_hyphen = !host.ends_with('-');
    let has_at_least_one_hyphen = host.contains('-');
    let hyphens_are_separated_by_valid_chars =
        has_at_least_one_hyphen.then(|| host.split('-').all(|part| part.chars().all(|c| c == '.' || c.is_ascii_alphanumeric())));
    let tld = host.split('.').last();
    // Prevents e.g. numbers being used as TLDs (which in turn prevents e.g. mistakes in IPv4 addresses as being detected as a domain).
    let tld_exists_and_is_not_number = tld.map(|tld| tld.parse::<i32>().is_err()).unwrap_or(false);

    if does_not_start_with_dot
        && does_not_end_with_dot
        && has_at_least_one_dot
        && dots_are_separated_by_valid_chars
        && does_not_start_with_hyphen
        && does_not_end_with_hyphen
        && hyphens_are_separated_by_valid_chars.unwrap_or(true)
        && tld_exists_and_is_not_number
    {
        return Ok(ParseHostOutput { scheme, host: Host::Domain(host), port, path });
    }

    Err(ParseHostError::InvalidInput)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_ip_v4_loopback() {
        let input = "127.0.0.1";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(output.port, None);
    }

    #[test]
    fn parse_host_ip_v4_invalid() {
        let input = "127.0.0.256";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn parse_host_ip_v4_with_port() {
        let input = "127.0.0.1:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_ip_v4_with_port_and_loopback() {
        let input = "ws://127.0.0.1:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, Some("ws"));
        assert_eq!(output.host, Host::Ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_ip_v6_loopback() {
        let input = "::1";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert_eq!(output.port, None);
    }

    #[test]
    fn parse_host_ip_v6_loopback_brackets() {
        let input = "[::1]";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert_eq!(output.port, None);
    }

    #[test]
    fn parse_host_ip_v6_invalid() {
        let input = "::g";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn parse_host_ip_v6_loopback_with_port() {
        let input = "[::1]:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_ip_v6_loopback_with_port_and_scheme() {
        let input = "ws://[::1]:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, Some("ws"));
        assert_eq!(output.host, Host::Ipv6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_domain() {
        let input = "example.com";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Domain("example.com"));
        assert_eq!(output.port, None);
    }

    #[test]
    fn parse_host_domain_with_port() {
        let input = "example.com:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Domain("example.com"));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_domain_with_port_and_scheme() {
        let input = "ws://example.com:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, Some("ws"));
        assert_eq!(output.host, Host::Domain("example.com"));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_hostname() {
        let input = "example";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Hostname("example"));
        assert_eq!(output.port, None);
    }

    #[test]
    fn parse_host_hostname_with_port() {
        let input = "example:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Hostname("example"));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn parse_host_hostname_with_port_and_scheme() {
        let input = "ws://example:8080";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, Some("ws"));
        assert_eq!(output.host, Host::Hostname("example"));
        assert_eq!(output.port, Some(8080));
    }

    #[test]
    fn should_fail_empty() {
        let input = "";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn should_fail_only_scheme() {
        let input = "ws://";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn should_fail_invalid_port() {
        let input = "example.com:808080";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert!(matches!(err, ParseHostError::ParsePortError(_)));
    }

    #[test]
    fn should_fail_only_port() {
        let input = ":8080";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn should_fail_only_numbers_domain() {
        let input = "123.123";
        let output = parse_host(input);
        assert!(output.is_err());
        let err = output.unwrap_err();
        assert_eq!(err, ParseHostError::InvalidInput);
    }

    #[test]
    fn numbers_domain() {
        let input = "123.com";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Domain("123.com"));
        assert_eq!(output.port, None);
    }

    #[test]
    fn wrpc_parse_mixed_subdomains() {
        let input = "alpha-123.beta.gamma.com";
        let output = parse_host(input).unwrap();
        assert_eq!(output.scheme, None);
        assert_eq!(output.host, Host::Domain("alpha-123.beta.gamma.com"));
        assert_eq!(output.port, None);
    }
}
