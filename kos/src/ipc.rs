use std::fmt;

#[derive(Clone, Debug)]
pub enum Modules {
    Core,
    Terminal,
    Metrics,
}

impl fmt::Display for Modules {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Modules::Core => write!(f, "core"),
            Modules::Terminal => write!(f, "terminal"),
            Modules::Metrics => write!(f, "metrics"),
        }
    }
}
