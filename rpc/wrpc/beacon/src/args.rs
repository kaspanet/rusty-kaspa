pub use clap::Parser;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// HTTP server port
    #[arg(long, default_value = "127.0.0.1:8888")]
    pub listen: String,

    /// Optional rate limit in the form `<requests>:<seconds>`, where `requests` is the number of requests allowed per specified number of `seconds`
    #[arg(long = "rate-limit", value_name = "REQUESTS:SECONDS")]
    pub rate_limit: Option<RateLimit>,

    /// Verbose mode
    #[arg(short, long, default_value = "false")]
    pub verbose: bool,

    /// Enable beacon status access via `/status`
    #[arg(long, default_value = "false")]
    pub status: bool,
}

#[derive(Clone, Debug)]
pub struct RateLimit {
    pub requests: u64,
    pub period: u64,
}

impl FromStr for RateLimit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err("invalid rate limit, must be `<requests>:<period>`".to_string());
        }
        let requests = parts[0]
            .parse()
            .map_err(|_| format!("Unable to parse number of requests, the value must be an integer, supplied: {:?}", parts[0]))?;
        let period = parts[1].parse().map_err(|_| {
            format!("Unable to parse period, the value must be an integer specifying number of seconds, supplied: {:?}", parts[1])
        })?;
        Ok(RateLimit { requests, period })
    }
}
