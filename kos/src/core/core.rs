use crate::imports::*;

#[derive(Debug, Clone)]
pub struct Terminal {
    #[allow(dead_code)]
    window: Arc<nw_sys::Window>,
    ipc: TerminalIpc,
}

impl Terminal {
    fn new(window: Arc<nw_sys::Window>) -> Self {
        Terminal { ipc: TerminalIpc::new(window.clone().into()), window }
    }

    #[allow(dead_code)]
    pub fn window(&self) -> &Arc<nw_sys::Window> {
        &self.window
    }

    pub fn ipc(&self) -> &TerminalIpc {
        &self.ipc
    }
}

/// Global application object created on application initialization.
static mut CORE: Option<Arc<Core>> = None;

/// Application struct wrapping `workflow_nw::Application` as an inner.
#[derive(Clone)]
pub struct Core {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<CoreOps>>,
    terminal: Arc<Mutex<Option<Arc<Terminal>>>>,
    pub kaspad: Arc<Kaspad>,
    pub cpu_miner: Arc<CpuMiner>,
    pub task_ctl: DuplexChannel,
    pub shutdown_ctl: Channel<()>,
}

unsafe impl Send for Core {}
unsafe impl Sync for Core {}

impl Core {
    /// Get access to the global application object
    #[allow(dead_code)]
    pub fn global() -> Option<Arc<Core>> {
        unsafe { CORE.clone() }
    }

    /// Create a new application instance
    pub async fn try_new() -> Result<Arc<Self>> {
        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_global_binding(Modules::Core)?,
            terminal: Arc::new(Mutex::new(Option::None)),
            kaspad: Arc::new(Kaspad::default()),
            cpu_miner: Arc::new(CpuMiner::default()),
            task_ctl: DuplexChannel::oneshot(),
            shutdown_ctl: Channel::oneshot(),
        });

        unsafe {
            CORE = Some(app.clone());
        };

        Ok(app)
    }

    pub fn terminal(&self) -> Arc<Terminal> {
        self.terminal.lock().unwrap().as_ref().unwrap().clone()
    }

    /// Create a test page window
    fn _create_window(&self) -> Result<()> {
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
        let modifier = if is_macos() { "command" } else { "ctrl" };

        let this = self.clone();
        let clipboard_copy = MenuItemBuilder::new()
            .label("Copy")
            // .key("c")
            // .modifiers(modifier)
            .callback(move |_| -> std::result::Result<(), JsValue> {
                let this = this.clone();
                spawn(async move {
                    this.terminal().ipc().clipboard_copy().await.unwrap_or_else(|e| log_error!("{}", e));
                });
                Ok(())
            })
            .build()?;

        let this = self.clone();
        let clipboard_paste = MenuItemBuilder::new()
            .label("Paste")
            // .key("v")
            // .modifiers(modifier)
            .callback(move |_| -> std::result::Result<(), JsValue> {
                let this = this.clone();
                spawn(async move {
                    this.terminal().ipc().clipboard_paste().await.unwrap_or_else(|e| log_error!("{}", e));
                });
                Ok(())
            })
            .build()?;

        let this = self.clone();
        let increase_font = MenuItemBuilder::new()
            .label("Increase Font")
            .key(if is_windows() { "=" } else { "+" })
            .modifiers(modifier)
            .callback(move |_| -> std::result::Result<(), JsValue> {
                // window().alert_with_message("Hello")?;
                let this = this.clone();
                spawn(async move {
                    this.terminal().ipc().increase_font_size().await.unwrap_or_else(|e| log_error!("{}", e));
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
                    this.terminal().ipc().decrease_font_size().await.unwrap_or_else(|e| log_error!("{}", e));
                });
                Ok(())
            })
            .build()?;

        let item = MenuItemBuilder::new()
            .label("Terminal")
            .submenus(vec![clipboard_copy, clipboard_paste, menu_separator(), increase_font, decrease_font])
            .build()?;

        MenubarBuilder::new("Kaspa OS", is_macos()).mac_hide_edit(true).mac_hide_window(true).append(item).build(true)?;

        Ok(())
    }

    /// Create application tray icon
    pub fn _create_tray_icon(&self) -> Result<()> {
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
    pub fn _create_tray_icon_with_menu(self: Arc<Self>) -> Result<()> {
        let this = self;
        let submenu_1 = MenuItemBuilder::new()
            .label("TEST IPC")
            .key("6")
            .modifiers("ctrl")
            .callback(move |_| -> std::result::Result<(), JsValue> {
                // window().alert_with_message("hi")?;

                let this = this.clone();

                spawn(async move {
                    let target = IpcTarget::new(this.terminal.lock().unwrap().as_ref().unwrap().window.as_ref());
                    let req = TestReq { req: "Hello World...".to_string() };
                    let _resp = target.call::<TermOps, TestReq, TestResp>(TermOps::TestTerminal, req).await;
                });

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

    fn register_ipc_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        self.ipc.method(
            CoreOps::KaspadCtl,
            Method::new(move |op: KaspadOps| {
                let this = this.clone();
                Box::pin(async move {
                    match op {
                        KaspadOps::Configure(config) => {
                            this.kaspad.configure(config)?;
                        }
                        KaspadOps::DaemonCtl(ctl) => match ctl {
                            DaemonCtl::Start => {
                                this.kaspad.start()?;
                            }
                            DaemonCtl::Stop => {
                                this.kaspad.stop()?;
                            }
                            DaemonCtl::Join => {
                                this.kaspad.join().await?;
                            }
                            DaemonCtl::Restart => {
                                this.kaspad.restart()?;
                            }
                            DaemonCtl::Kill => {
                                this.kaspad.kill()?;
                            }
                            DaemonCtl::Mute(mute) => {
                                this.kaspad.mute(mute).await?;
                            }
                            DaemonCtl::ToggleMute => {
                                this.kaspad.toggle_mute().await?;
                            }
                        },
                    }

                    Ok(())
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            CoreOps::KaspadStatus,
            Method::new(move |_op: ()| {
                let this = this.clone();
                Box::pin(async move {
                    let uptime = this.kaspad.uptime().map(|u| u.as_secs());
                    Ok(DaemonStatus { uptime })
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            CoreOps::KaspadVersion,
            Method::new(move |_op: ()| {
                let this = this.clone();
                Box::pin(async move {
                    let version = this.kaspad.version().await?;
                    Ok(version)
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            CoreOps::CpuMinerCtl,
            Method::new(move |op: CpuMinerOps| {
                let this = this.clone();
                Box::pin(async move {
                    match op {
                        CpuMinerOps::Configure(config) => {
                            this.cpu_miner.configure(config)?;
                        }
                        CpuMinerOps::DaemonCtl(ctl) => match ctl {
                            // DaemonCtl::Version => {
                            //     this.cpu_miner.version().await?;
                            // }
                            DaemonCtl::Start => {
                                this.cpu_miner.start()?;
                            }
                            DaemonCtl::Stop => {
                                this.cpu_miner.stop()?;
                            }
                            DaemonCtl::Join => {
                                this.cpu_miner.join().await?;
                            }
                            DaemonCtl::Restart => {
                                this.cpu_miner.restart()?;
                            }
                            DaemonCtl::Kill => {
                                this.cpu_miner.kill()?;
                            }
                            DaemonCtl::Mute(mute) => {
                                this.cpu_miner.mute(mute).await?;
                            }
                            DaemonCtl::ToggleMute => {
                                this.cpu_miner.toggle_mute().await?;
                            }
                        },
                    }

                    Ok(())
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            CoreOps::CpuMinerStatus,
            Method::new(move |_op: ()| {
                let this = this.clone();
                Box::pin(async move {
                    let uptime = this.kaspad.uptime().map(|u| u.as_secs());
                    Ok(DaemonStatus { uptime })
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            CoreOps::Shutdown,
            Method::new(move |_op: ()| {
                let this = this.clone();
                Box::pin(async move {
                    this.shutdown_ctl.send(()).await.unwrap_or_else(|err| log_error!("{}", err));
                    Ok(())
                })
            }),
        );

        Ok(())
    }

    pub async fn handle_event(self: &Arc<Self>, event: DaemonEvent) -> Result<()> {
        self.terminal().ipc().relay_event(event).await?;

        Ok(())
    }

    pub async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();
        let kaspad_events_receiver = self.kaspad.events().receiver.clone();
        let cpu_miner_events_receiver = self.cpu_miner.events().receiver.clone();

        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    event = kaspad_events_receiver.recv().fuse() => {
                        if let Ok(event) = event {
                            this.handle_event(DaemonEvent::new(DaemonKind::Kaspad, event)).await.unwrap_or_else(|err| {
                                log_error!("error while handling kaspad stdout: {err}");
                            });
                        }
                    },
                    event = cpu_miner_events_receiver.recv().fuse() => {
                        if let Ok(event) = event {
                            this.handle_event(DaemonEvent::new(DaemonKind::CpuMiner, event)).await.unwrap_or_else(|err| {
                                log_error!("error while handling cpu miner stdout: {err}");
                            });
                        }
                    },
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });

        Ok(())
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Core::stop_task() `signal` error");
        Ok(())
    }

    pub async fn init_terminal_window(self: &Arc<Self>) -> Result<()> {
        let window = Arc::new(
            Application::create_window_async(
                "/app/index.html",
                &nw_sys::window::Options::new().new_instance(false).height(768).width(1280),
            )
            .await?,
        );

        self.terminal.lock().unwrap().replace(Arc::new(Terminal::new(window)));

        Ok(())
    }

    pub async fn main(self: &Arc<Self>) -> Result<()> {
        self.register_ipc_handlers()?;

        self.init_terminal_window().await?;

        self.create_menu()?;

        self.start_task().await?;

        self.shutdown_ctl.recv().await?;

        self.stop_task().await?;

        Ok(())
    }
}

#[wasm_bindgen]
pub async fn init_core() -> Result<()> {
    workflow_wasm::panic::init_console_panic_hook();
    kaspa_core::log::set_log_level(LevelFilter::Info);

    let core = Core::try_new().await?;
    core.main().await?;

    nw_sys::app::quit();

    Ok(())
}
