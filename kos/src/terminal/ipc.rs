use crate::imports::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum TermOps {
    // TestBg,
    TestTerminal,
    FontCtl,
    Stdio,
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

    pub async fn pipe_stdout(&self, stdio: Stdio) -> Result<()> {
        self.target.notify(TermOps::Stdio, stdio).await?;
        Ok(())
    }
}
