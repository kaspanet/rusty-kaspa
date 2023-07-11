use crate::imports::*;

#[derive(Clone, Debug)]
pub enum Modules {
    Background,
    Terminal,
    Node,
}

impl ToString for Modules {
    fn to_string(&self) -> String {
        match self {
            Modules::Background => "background",
            Modules::Terminal => "terminal",
            Modules::Node => "node",
        }
        .to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum BgOps {
    TestBg,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum TermOps {
    TestTerminal,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct TestReq {
    pub req: String,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct TestResp {
    pub resp: String,
}

pub struct TerminalApi {
    target: Arc<nw_sys::Window>,
}

impl TerminalApi {
    pub fn new(target: Arc<nw_sys::Window>) -> TerminalApi {
        TerminalApi { target }
    }
}

pub struct BackgroundApi {
    target: Arc<nw_sys::Window>,
}

impl BackgroundApi {}
