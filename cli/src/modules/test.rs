use crate::imports::*;

use nw_sys::notifications;
use nw_sys::options::*;
// use kaspa_cli_lib::daemon::*;

#[derive(Default, Handler)]
#[help("Testing ...")]
pub struct Test;

impl Test {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, _argv: Vec<String>, _cmd: &str) -> Result<()> {
        tprintln!(ctx, "testing...");
        use wasm_bindgen::prelude::*;

        // let closure = Closure::new(move |v : JsValue| {
        //     tprintln!(ctx, "testing...");

        // });
        let ctx = ctx.clone();
        let closure = Closure::wrap(Box::new(move |p: JsValue| {
            tprintln!(ctx, "permissions: {:?}", p);
            // Call the double_value function and get the result
            // let result = double_value(event);

            // Convert the result back to a string and log it
            // let result_str = result.as_f64().map_or("NaN".to_string(), |num| num.to_string());
            // log(&format!("Result: {}", result_str));
        }) as Box<dyn FnMut(JsValue)>);

        notifications::get_permission_level(closure.as_ref().unchecked_ref());
        closure.forget();

        // Create image notification
        let options = notifications::Options::new()
            .title("Title text")
            .icon_url("/resources/icons/tray-icon@2x.png")
            .set_type(notifications::TemplateType::Basic)
            .message("Message Text")
            .context_message("Context Message");
        notifications::create(Some("abc".to_string()), &options, None);

        // let sp = nw_sys::app::start_path();
        // let dp = nw_sys::app::data_path();

        // tprintln!(ctx, "start path: {}", sp);
        // tprintln!(ctx, "data path: {}", dp);

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
