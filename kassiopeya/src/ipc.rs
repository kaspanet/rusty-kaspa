use crate::imports::*;

#[derive(Clone, Debug)]
pub enum Modules {
    Background,
    Terminal,
    // Node,
}

impl ToString for Modules {
    fn to_string(&self) -> String {
        match self {
            Modules::Background => "background",
            Modules::Terminal => "terminal",
            // Modules::Node => "node",
        }
        .to_string()
    }
}

// #[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
// pub struct NoResp;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum BgOps {
    TestBg,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum TermOps {
    TestTerminal,
    FontCtl,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct TestReq {
    pub req: String,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct TestResp {
    pub resp: String,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum FontCtl {
    IncreaseSize,
    DecreaseSize,
    // SetSize(u32),
}

#[derive(Debug, Clone)]
pub struct TerminalIpc {
    target: IpcTarget,
}

impl TerminalIpc {
    pub fn new(target: IpcTarget) -> TerminalIpc {
        TerminalIpc { target }
    }

    pub async fn increase_font_size(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::FontCtl, FontCtl::IncreaseSize).await?;
        Ok(())
    }

    pub async fn decrease_font_size(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::FontCtl, FontCtl::DecreaseSize).await?;
        Ok(())
    }
}

// pub struct BackgroundApi {
//     target: Arc<nw_sys::Window>,
// }

// impl BackgroundApi {}
