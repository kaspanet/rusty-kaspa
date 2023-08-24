use crate::imports::*;
use application_runtime::*;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSettings {
    #[describe("Theme name")]
    Name,
}

#[async_trait]
impl DefaultSettings for ThemeSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![(Self::Name, to_value("default").unwrap())]
    }
}

pub struct Theme {
    settings: SettingsStore<ThemeSettings>,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { settings: SettingsStore::try_new("theme").expect("Failed to create theme settings store") }
    }
}

#[async_trait]
impl Handler for Theme {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        (is_nw() || is_web()).then_some("theme")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Change application theme"
    }

    async fn start(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.settings.try_load().await.ok();
        if let Some(_name) = self.settings.get::<String>(ThemeSettings::Name) {
            // TODO - set theme
        }
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Theme {
    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        let help = "\n\
        \n\
        ";

        tprintln!(ctx, "{}", help.crlf());

        Ok(())
    }
}
