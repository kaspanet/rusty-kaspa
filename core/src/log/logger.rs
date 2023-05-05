use log::LevelFilter;
use log4rs::config::Logger;
use std::{collections::HashMap, env, mem, str::FromStr};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum LogError {
    #[error("Logger spec parsing error: {0}")]
    ParseLoggerSpecError(String),
}

#[derive(Clone)]
pub(super) struct LoggerSpec {
    pub name: String,
    pub level: LevelFilter,
    pub appenders: Vec<&'static str>,
}

impl LoggerSpec {
    pub fn new(name: String, level: LevelFilter, appenders: Vec<&'static str>) -> Self {
        Self { name, level, appenders }
    }

    pub fn logger(&self) -> Logger {
        Logger::builder().appenders(self.appenders.iter().map(|x| x.to_string())).build(self.name.clone(), self.level)
    }
}

pub(super) struct Loggers {
    loggers: Vec<LoggerSpec>,
    root_level: LevelFilter,
}

impl Loggers {
    pub fn root_level(&self) -> LevelFilter {
        self.root_level
    }

    pub fn items(&self) -> impl IntoIterator<Item = Logger> + '_ {
        self.loggers.iter().map(|x| x.logger())
    }
}

pub(super) struct Builder {
    appenders: Vec<&'static str>,
    loggers: HashMap<String, (Vec<&'static str>, LevelFilter)>,
    root_level: Option<LevelFilter>,
}

impl Builder {
    pub fn new() -> Builder {
        Builder { appenders: vec![], loggers: HashMap::new(), root_level: None }
    }

    /// Initializes the builder from an environment variable.
    #[allow(dead_code)]
    pub fn from_env(env: &str) -> Self {
        let mut builder = Self::new();
        builder.parse_env(env);
        builder
    }

    pub fn parse_env(&mut self, env: &str) -> &mut Self {
        self.parse_expression(&env::var(env).unwrap_or_default())
    }

    /// Initializes the builder from a specs expression.
    #[allow(dead_code)]
    pub fn from_expression(expression: &str) -> Self {
        let mut builder = Self::new();
        builder.parse_expression(expression);
        builder
    }

    pub fn parse_expression(&mut self, expression: &str) -> &mut Self {
        self.parse_specs(expression)
    }

    fn parse_specs(&mut self, expression: &str) -> &mut Self {
        for spec in expression.split(',').map(|x| x.trim()) {
            if spec.is_empty() {
                continue;
            }
            let mut parts = spec.split('=');
            let (log_level, name) = match (parts.next(), parts.next().map(|x| x.trim()), parts.next()) {
                (Some(part0), None, None) => {
                    // if the single argument is a log-level string or number,
                    // it defines the root level
                    match part0.parse() {
                        Ok(lvl) => (lvl, None),
                        Err(_) => (LevelFilter::max(), Some(part0)),
                    }
                }
                (Some(part0), Some(""), None) => (LevelFilter::max(), Some(part0)),
                (Some(part0), Some(part1), None) => match part1.parse() {
                    Ok(lvl) => (lvl, Some(part0)),
                    _ => {
                        println!("Ignoring invalid logging spec '{}'", LogError::ParseLoggerSpecError(part1.to_string()));
                        continue;
                    }
                },
                _ => {
                    println!("Ignoring invalid logging spec '{}'", LogError::ParseLoggerSpecError(spec.to_string()));
                    continue;
                }
            };
            match name {
                Some(name) => {
                    self.logger(name.to_string(), log_level);
                }
                None => {
                    self.root_level(log_level);
                }
            }
        }
        self
    }

    #[allow(dead_code)]
    pub fn appenders(&mut self, appenders: impl Iterator<Item = &'static str>) -> &mut Self {
        self.appenders = appenders.collect();
        self
    }

    pub fn root_level(&mut self, root_level: LevelFilter) -> &mut Self {
        self.root_level.replace(root_level);
        self
    }

    pub fn logger(&mut self, name: String, level: LevelFilter) -> &mut Self {
        self.loggers.insert(name, (self.appenders.clone(), level));
        self
    }

    pub fn build(&mut self) -> Loggers {
        let loggers_map = mem::take(&mut self.loggers);
        let loggers =
            loggers_map.into_iter().map(|(name, (appenders, level))| LoggerSpec::new(name, level, appenders)).collect::<Vec<_>>();
        Loggers { loggers, root_level: self.root_level.take().unwrap_or(LevelFilter::Error) }
    }
}

impl FromStr for Builder {
    type Err = LogError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_expression(s))
    }
}
