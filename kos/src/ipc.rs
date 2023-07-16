#[derive(Clone, Debug)]
pub enum Modules {
    Core,
    Terminal,
    // Node,
}

impl ToString for Modules {
    fn to_string(&self) -> String {
        match self {
            Modules::Core => "core",
            Modules::Terminal => "terminal",
            // Modules::Node => "node",
        }
        .to_string()
    }
}
