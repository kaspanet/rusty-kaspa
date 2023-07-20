use crate::imports::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum TermOps {
    // TestBg,
    TestTerminal,
    FontCtl,
    EditCtl,
    Stdio,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum FontCtl {
    IncreaseSize,
    DecreaseSize,
    // SetSize(u32),
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum EditCtl {
    Copy,
    Paste,
}

#[derive(Debug, Clone)]
pub struct TerminalIpc {
    target: IpcTarget,
}

impl TerminalIpc {
    pub fn new(target: IpcTarget) -> TerminalIpc {
        TerminalIpc { target }
    }

    pub async fn clipboard_copy(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::EditCtl, EditCtl::Copy).await?;
        Ok(())
    }

    pub async fn clipboard_paste(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::EditCtl, EditCtl::Paste).await?;
        Ok(())
    }

    pub async fn increase_font_size(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::FontCtl, FontCtl::IncreaseSize).await?;
        Ok(())
    }

    pub async fn decrease_font_size(&self) -> Result<()> {
        self.target.call::<_, _, ()>(TermOps::FontCtl, FontCtl::DecreaseSize).await?;
        Ok(())
    }

    pub async fn pipe_stdio(&self, stdio: Stdio) -> Result<()> {
        self.target.notify(TermOps::Stdio, stdio).await?;
        Ok(())
    }
}
