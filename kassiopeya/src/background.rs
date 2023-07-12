// use nw_sys::window::Options;
// use workflow_nw::ipc::*;
//{imports::{ResponseError, Generator}, IpcDispatch};
// use workflow_wasm::prelude::Callback;

use crate::imports::*;
// use crate::result::Result;
// use kaspa_cli::kaspa_cli;
// use nw_sys::prelude::*;
// use wasm_bindgen::prelude::*;
// use workflow_log::{log_info, log_trace};
// use workflow_nw::prelude::*;
// use workflow_terminal::Options as TerminalOptions;

/// Global application object created on application initialization.
static mut BACKGROUND: Option<Arc<Background>> = None;

/// Application struct wrapping `workflow_nw::Application` as an inner.
#[derive(Clone)]
pub struct Background {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<BgOps>>,
    pub terminal: Arc<Mutex<Option<Arc<nw_sys::Window>>>>,
}

unsafe impl Send for Background {}
unsafe impl Sync for Background {}

impl Background {
    /// Get access to the global application object
    #[allow(dead_code)]
    pub fn global() -> Option<Arc<Background>> {
        unsafe { BACKGROUND.clone() }
    }

    /// Create a new application instance
    pub async fn try_new() -> Result<Arc<Self>> {
        // let options = nw_sys::window::Options::new().show(false);

        // let window = Arc::new(Application::create_window_async("blank.html", &options).await?);

        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_global_binding(Modules::Background)?,
            terminal: Arc::new(Mutex::new(Option::None)),
        });

        unsafe {
            BACKGROUND = Some(app.clone());
        };

        Ok(app)
    }

    pub fn terminal_window(&self) -> Arc<nw_sys::Window> {
        self.terminal.lock().unwrap().as_ref().unwrap().clone()
    }

    pub fn terminal_ipc(&self) -> TerminalIpc {
        TerminalIpc::new(self.terminal_window().into())
    }

    /// Create a test page window
    fn create_window(&self) -> Result<()> {
        let options = nw_sys::window::Options::new().title("Test page").width(200).height(200).left(0);

        self.inner.create_window_with_callback(
            "/root/page2.html",
            &options,
            |win: nw_sys::window::Window| -> std::result::Result<(), JsValue> {
                log_trace!("win: {:?}", win);
                log_trace!("win.x: {:?}", win.x());
                win.move_by(300, 0);
                win.set_x(100);
                win.set_y(100);

                log_trace!("win.title: {}", win.title());
                win.set_title("Another Window");
                log_trace!("win.set_title(\"Another Window\")");
                log_trace!("win.title: {}", win.title());

                Ok(())
            },
        )?;

        Ok(())
    }

    /// Create application menu
    fn create_menu(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let submenu_1 = MenuItemBuilder::new()
            .label("Create window")
            .key("8")
            .modifiers("ctrl")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                log_trace!("Create window : menu clicked");
                this.create_window()?;
                Ok(())
            })
            .build()?;

        let submenu_2 = MenuItemBuilder::new()
            .label("Say hello")
            .key("9")
            .modifiers("ctrl")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                window().alert_with_message("Hello")?;
                Ok(())
            })
            .build()?;

        let modifier = if is_macos() { "command" } else { "ctrl" };

        let this = self.clone();
        let increase_font = MenuItemBuilder::new()
            .label("Increase Font")
            .key("+")
            .modifiers(modifier)
            .callback(move |_| -> std::result::Result<(), JsValue> {
                // window().alert_with_message("Hello")?;
                let this = this.clone();
                spawn(async move {
                    this.terminal_ipc().increase_font_size().await.unwrap_or_else(|e| log_error!("{}", e));
                });
                Ok(())
            })
            .build()?;

        let this = self.clone();
        let decrease_font = MenuItemBuilder::new()
            .label("Decrease Font")
            .key("-")
            .modifiers(modifier)
            .callback(move |_| -> std::result::Result<(), JsValue> {
                // window().alert_with_message("Hello")?;
                let this = this.clone();
                spawn(async move {
                    this.terminal_ipc().decrease_font_size().await.unwrap_or_else(|e| log_error!("{}", e));
                });
                Ok(())
            })
            .build()?;

        let item = MenuItemBuilder::new()
            .label("Top Menu")
            .submenus(vec![submenu_1, menu_separator(), submenu_2, menu_separator(), increase_font, decrease_font])
            .build()?;

        MenubarBuilder::new("Kassiopeya").mac_hide_edit(true).mac_hide_window(true).append(item).build(true)?;

        Ok(())
    }

    /// Create application tray icon
    pub fn create_tray_icon(&self) -> Result<()> {
        let _tray = TrayMenuBuilder::new()
            .icon("resources/icons/tray-icon@2x.png")
            .icons_are_templates(false)
            .callback(|_| {
                window().alert_with_message("Tray Icon click")?;
                Ok(())
            })
            .build()?;
        Ok(())
    }

    /// Create application tray icon and tray menu
    pub fn create_tray_icon_with_menu(self: Arc<Self>) -> Result<()> {
        let this = self.clone();
        let submenu_1 = MenuItemBuilder::new()
            .label("TEST IPC")
            .key("6")
            .modifiers("ctrl")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                // window().alert_with_message("hi")?;
                // cfg_if::cfg_if! {
                // if #[cfg(target_arch = "wasm32")] {

                let this = this.clone();

                spawn(async move {
                    // let source = this.ipc.window();
                    log_info!("CALLING IPC");
                    let target = IpcTarget::new(this.terminal.lock().unwrap().as_ref().cloned().unwrap().as_ref());
                    log_info!("TARGET");

                    let req = TestReq { req: "Hello World...".to_string() };
                    log_info!("CALLING");
                    let resp = target
                        .call::<TermOps, TestReq, TestResp>(
                            TermOps::TestTerminal,
                            req,
                            // source
                        )
                        .await;
                    log_info!("AWAIT IS FINISHED: {:?}", resp);
                });

                // use workflow_terminal::bindings::*;
                // use workflow_terminal::xterm::*;

                // log_info!("A");
                // let xterm = kaspa_cli::term();
                // log_info!("B");
                // let xterm = xterm.as_ref();
                // log_info!("C");
                // let xtermx = xterm.unwrap().term();
                // {
                //     log_info!("D");
                //     let xterm = xtermx.xterm();
                //     log_info!("E");
                //     let xterm_impl = xterm.as_ref();
                //     log_info!("F");
                //     let xterm_impl = xterm_impl.expect("Could not get xterm");
                //     log_info!("G");
                //     log_info!("setting fontSize");
                //     xterm_impl.set_option("fontSize", JsValue::from(8));
                //     log_info!("updating rows...");
                // }
                // xtermx.resize();
                // }
                // }

                Ok(())
            })
            .build()?;

        let exit_menu = MenuItemBuilder::new()
            .label("Exit")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                window().alert_with_message("TODO: Exit")?;
                Ok(())
            })
            .build()?;

        let _tray = TrayMenuBuilder::new()
            .icon("resources/icons/tray-icon@2x.png")
            .icons_are_templates(false)
            .submenus(vec![submenu_1, menu_separator(), exit_menu])
            .build()?;

        Ok(())
    }

    /// Create a custom application context menu
    #[allow(dead_code)]
    pub fn create_context_menu(self: Arc<Self>) -> Result<()> {
        let item_1 = MenuItemBuilder::new()
            .label("Sub Menu 1")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                window().alert_with_message("Context menu 1 clicked")?;
                Ok(())
            })
            .build()?;

        let item_2 = MenuItemBuilder::new()
            .label("Sub Menu 2")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                window().alert_with_message("Context menu 2 clicked")?;
                Ok(())
            })
            .build()?;

        self.inner.create_context_menu(vec![item_1, item_2])?;

        Ok(())
    }
}

/// Creates the application context menu
// #[wasm_bindgen]
// pub fn create_context_menu() -> Result<()> {
//     if let Some(app) = App::global() {
//         app.create_context_menu()?;
//     } else {
//         let is_nw = initialize_app()?;
//         if !is_nw {
//             log_info!("TODO: initialize web-app");
//             return Ok(());
//         }
//         let app = App::global().expect("Unable to create app");
//         app.create_context_menu()?;
//     }
//     Ok(())
// }

// /// Crteates the application instance
// #[wasm_bindgen]
// pub fn initialize_app() -> Result<bool> {
//     let is_nw = is_nw();

//     let _app = App::new()?;
//     Ok(is_nw)
// }

/// This function is called from the main `/index.js` file
/// and creates the main application window containing
/// `index.html`
#[wasm_bindgen]
pub async fn init_background() -> Result<()> {
    workflow_wasm::panic::init_console_panic_hook();

    // let options = nw_sys::window::Options::new().title("Background page").width(200).height(200).left(0);

    // let background_window = Application::create_window_async("blank.html", &options).await?;

    // log_info!("getting background window");
    // let window = nw_sys::window::get();
    // log_info!("window: {:?}", window);

    let app = Background::try_new().await?; //background_window)?;

    // app.ipc.lock().unwrap().method(Ops::TestBg, Method::new(move |args : TestReq| {
    //     Box::pin(async move {

    //         let resp : TestResp = TestResp { resp : args.req+" - response!" };

    //         Ok(resp)
    //         // manager.start_notify(&connection, scope).await.map_err(|err| err.to_string())?;
    //         // Ok(SubscribeResponse::new(connection.id()))
    //     })
    // }));

    // app.ipc.method(BackgroundOps::TestBg, Method::new(
    //     Box::pin(async move {
    //         manager.start_notify(&connection, scope).await.map_err(|err| err.to_string())?;
    //         Ok(SubscribeResponse::new(connection.id()))
    //     })
    // }));
    //     Box::new(move |args: Vec<Value>| -> Result<Value> {

    // }));

    // let app = App::global().expect("Unable to create background application instance");

    let window = Application::create_window_async(
        "/app/index.html",
        &nw_sys::window::Options::new().new_instance(false).height(768).width(1280),
        // |_win: Window| -> std::result::Result<(), JsValue> {
        //     //app.create_context_menu()?;
        //     Ok(())
        // },
    )
    .await?;

    app.terminal.lock().unwrap().replace(Arc::new(window));

    // app.inner.create_window_with_callback(
    //     "/app/index.html",
    //     &window::Options::new().new_instance(false).height(768).width(1280),
    //     |_win: Window| -> std::result::Result<(), JsValue> {
    //         //app.create_context_menu()?;
    //         Ok(())
    //     },
    // )?;

    // let window = window::get();
    // log_trace!("nw.Window.get(): {:?}", window);

    app.create_menu()?;
    app.create_tray_icon()?;
    app.create_tray_icon_with_menu()?;

    Ok(())
}

// #[wasm_bindgen]
// pub async fn initialize_kassiopeya_application() -> Result<()> {
//     workflow_wasm::panic::init_console_panic_hook();

//     App::new()?;

//     let app = App::global().expect("Unable to create app");

//     app.create_menu()?;
//     app.create_tray_icon()?;
//     app.create_tray_icon_with_menu()?;

//     let options = TerminalOptions { ..TerminalOptions::default() };
//     let banner = format!("Kaspa OS v{} (type 'help' for list of commands)", env!("CARGO_PKG_VERSION"));
//     kaspa_cli(options, Some(banner)).await?;
//     Ok(())
// }
