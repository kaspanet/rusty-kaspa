use crate::imports::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum TermOps {
    TestTerminal,
    FontCtl,
    EditCtl,
    DaemonEvent,
    MetricsCtl,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum FontCtl {
    IncreaseSize,
    DecreaseSize,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum EditCtl {
    Copy,
    Paste,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsSinkCtl {
    Activate,
    Deactivate,
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

    pub async fn relay_event(&self, event: DaemonEvent) -> Result<()> {
        self.target.notify(TermOps::DaemonEvent, event).await?;
        Ok(())
    }

    pub async fn metrics_ctl(&self, ctl: MetricsSinkCtl) -> Result<()> {
        self.target.notify(TermOps::MetricsCtl, ctl).await?;
        Ok(())
    }
}
