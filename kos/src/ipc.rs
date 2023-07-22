#[derive(Clone, Debug)]
pub enum Modules {
    Core,
    Terminal,
    Metrics,
}

impl ToString for Modules {
    fn to_string(&self) -> String {
        match self {
            Modules::Core => "core",
            Modules::Terminal => "terminal",
            Modules::Metrics => "metrics",
        }
        .to_string()
    }
}
