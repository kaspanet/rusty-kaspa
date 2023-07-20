use crate::imports::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum CoreOps {
    TestBg,
    Shutdown,
    KaspadCtl,
    KaspadStatus,
    KaspadVersion,
    CpuMinerCtl,
    CpuMinerStatus,
    CpuMinerVersion,
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
pub enum DaemonCtl {
    Start,
    Stop,
    Join,
    Restart,
    Kill,
    Mute(bool),
    ToggleMute,
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum KaspadOps {
    Configure(KaspadConfig),
    DaemonCtl(DaemonCtl),
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum CpuMinerOps {
    Configure(CpuMinerConfig),
    DaemonCtl(DaemonCtl),
}

#[derive(Debug, Clone)]
pub struct CoreIpc {
    target: IpcTarget,
}

impl CoreIpc {
    pub fn new(target: IpcTarget) -> CoreIpc {
        CoreIpc { target }
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.target.call(CoreOps::Shutdown, ()).await?;
        Ok(())
    }
}

#[async_trait]
impl KaspadCtl for CoreIpc {
    async fn configure(&self, config: KaspadConfig) -> DaemonResult<()> {
        // self.target.call::<_, _, ()>(CoreOps::KaspadCtl, KaspadOps::Configure(config)).await?;
        self.target.call(CoreOps::KaspadCtl, KaspadOps::Configure(config)).await?;

        Ok(())
    }

    async fn start(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Start)).await?;
        Ok(())
    }

    async fn stop(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Stop)).await?;
        Ok(())
    }

    async fn join(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Join)).await?;
        Ok(())
    }

    async fn restart(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Restart)).await?;
        Ok(())
    }

    async fn kill(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Kill)).await?;
        Ok(())
    }

    async fn status(&self) -> DaemonResult<DaemonStatus> {
        Ok(self.target.call(CoreOps::KaspadStatus, ()).await?)
    }

    async fn version(&self) -> DaemonResult<String> {
        Ok(self.target.call(CoreOps::KaspadVersion, ()).await?)
    }

    async fn mute(&self, mute: bool) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::Mute(mute))).await?;
        Ok(())
    }

    async fn toggle_mute(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::KaspadCtl, KaspadOps::DaemonCtl(DaemonCtl::ToggleMute)).await?;
        Ok(())
    }
}

#[async_trait]
impl CpuMinerCtl for CoreIpc {
    async fn configure(&self, config: CpuMinerConfig) -> DaemonResult<()> {
        // self.target.call::<_, _, ()>(CoreOps::KaspadCtl, KaspadOps::Configure(config)).await?;
        self.target.call(CoreOps::CpuMinerCtl, CpuMinerOps::Configure(config)).await?;

        Ok(())
    }

    async fn start(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Start)).await?;
        Ok(())
    }

    async fn stop(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Stop)).await?;
        Ok(())
    }

    async fn join(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Join)).await?;
        Ok(())
    }

    async fn restart(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Restart)).await?;
        Ok(())
    }

    async fn kill(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Kill)).await?;
        Ok(())
    }

    async fn status(&self) -> DaemonResult<DaemonStatus> {
        Ok(self.target.call(CoreOps::CpuMinerStatus, ()).await?)
    }

    async fn version(&self) -> DaemonResult<String> {
        Ok(self.target.call(CoreOps::CpuMinerVersion, ()).await?)
    }

    async fn mute(&self, mute: bool) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::Mute(mute))).await?;
        Ok(())
    }

    async fn toggle_mute(&self) -> DaemonResult<()> {
        self.target.call(CoreOps::CpuMinerCtl, KaspadOps::DaemonCtl(DaemonCtl::ToggleMute)).await?;
        Ok(())
    }
}
