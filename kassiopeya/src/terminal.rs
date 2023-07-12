use workflow_nw::ipc::ipc::get_ipc_target;

use crate::imports::*;

static mut APP: Option<Arc<App>> = None;

/// Application struct wrapping `workflow_nw::Application` as an inner.
#[derive(Clone)]
pub struct App {
    pub inner: Arc<Application>,
    // pub background: Arc<Mutex<Ipc<BackgroundOps>>>,
    pub ipc: Arc<Ipc<TermOps>>,
    pub background: IpcTarget,
}

impl App {
    /// Get access to the global application object
    pub fn global() -> Option<Arc<App>> {
        unsafe { APP.clone() }
    }

    /// Create a new application instance
    pub async fn try_new() -> Result<Arc<Self>> {
        let background = get_ipc_target(Modules::Background).await?.expect("Unable to aquire background window");
        // log_info!("+++ FOUND Background window: {:?}", background);
        let window = Arc::new(nw_sys::window::get());

        let app =
            Arc::new(Self { inner: Application::new()?, ipc: Ipc::try_new_window_binding(&window, Modules::Terminal)?, background });

        unsafe {
            APP = Some(app.clone());
        };

        Ok(app)
    }
}

#[wasm_bindgen]
pub async fn init_application() -> Result<()> {
    workflow_wasm::panic::init_console_panic_hook();

    App::try_new().await?;

    KaspaCli::init();
    let options = TerminalOptions { ..TerminalOptions::default() };
    let cli = KaspaCli::try_new_arc(options).await?;

    let app = App::global().expect("Unable to create app");

    app.ipc.method(
        TermOps::TestTerminal,
        Method::new(move |args: TestReq| {
            Box::pin(async move {
                let resp: TestResp = TestResp { resp: args.req + " - response from terminal!" };

                Ok(resp)
                // manager.start_notify(&connection, scope).await.map_err(|err| err.to_string())?;
                // Ok(SubscribeResponse::new(connection.id()))
            })
        }),
    );

    let cli_ = cli.clone();
    app.ipc.method(
        TermOps::FontCtl,
        Method::new(move |args: FontCtl| {
            let cli_ = cli_.clone();
            Box::pin(async move {
                // let resp: TestResp = TestResp { resp: args.req + " - response from terminal!" };
                match args {
                    // FontCtl::SetFont(font) => {
                    //     log_info!("Setting font to: {:?}", font);
                    // },
                    FontCtl::IncreaseSize => {
                        cli_.term().increase_font_size().map_err(|e| e.to_string())?;
                        //writeln("Font size increased!");
                    }
                    FontCtl::DecreaseSize => {
                        cli_.term().decrease_font_size().map_err(|e| e.to_string())?;
                        //writeln("Font size increased!");
                        // cli_.term().writeln("Font size decreased!");
                    }
                }
                Ok(())
                // manager.start_notify(&connection, scope).await.map_err(|err| err.to_string())?;
                // Ok(SubscribeResponse::new(connection.id()))
            })
        }),
    );

    // app.create_menu()?;
    // app.create_tray_icon()?;
    // app.create_tray_icon_with_menu()?;

    // redirect the global log output to terminal
    // #[cfg(not(target_arch = "wasm32"))]
    // workflow_log::pipe(Some(cli.clone()));

    cli.register_handlers()?;

    use crate::modules::*;
    register_handlers!(cli, cli.handlers(), [test]);

    // cli.handlers().register(&cli, crate::modules::test::Test::default());

    // cli starts notification->term trace pipe task
    cli.start().await?;

    let banner = format!("Kaspa OS v{} (type 'help' for list of commands)", env!("CARGO_PKG_VERSION"));
    cli.term().writeln(banner);

    // terminal blocks async execution, delivering commands to the terminals
    cli.run().await?;

    // cli stops notification->term trace pipe task
    cli.stop().await?;

    Ok(())
}
