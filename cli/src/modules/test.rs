use crate::imports::*;
// use kaspa_cli::daemon::*;

#[derive(Default, Handler)]
#[help("Testing ...")]
pub struct Test;

impl Test {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(ctx, "testing...");

        let sp = nw_sys::app::start_path();
        let dp = nw_sys::app::data_path();

        tprintln!(ctx, "start path: {}", sp);
        tprintln!(ctx, "data path: {}", dp);

        // let list = locate_binaries(sp.as_str(), "kaspad").await?;
        // for item in list {
        //     tprintln!(ctx, "location: {}", item.as_os_str().to_str().unwrap());
        // }
        // nw_sys::app::manifest();

        // let theme = Theme {
        //     foreground : Some("red".to_string()),
        //     background : Some("white".to_string()),
        //     ..Default::default()
        // };
        // // theme.foreground = Some("red".to_string());
        // ctx.term().set_theme(theme)?;

        Ok(())
    }
}
