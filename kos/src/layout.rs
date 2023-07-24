use crate::imports::*;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Position {
    x: u32,
    y: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Size {
    width: u32,
    height: u32,
}

pub struct Layout<S>
where
    S: SettingsStoreT,
{
    pub callbacks: CallbackMap,
    pub settings: Arc<S>,
    pub window: Arc<nw_sys::window::Window>,
}

impl<S> Layout<S>
where
    S: SettingsStoreT,
{
    pub async fn try_new(window: &Arc<nw_sys::window::Window>, settings: &Arc<S>) -> Result<Self> {
        let callbacks = CallbackMap::new();

        let settings_ = settings.clone();
        let move_window = callback!(move |x: u32, y: u32| {
            let settings_ = settings_.clone();
            let position = Position { x, y };
            log_info!("Move window to ({}, {})", position.x, position.x);
            spawn(async move {
                settings_.set("window.position", position).await.unwrap_or_else(|err| {
                    log_error!("Unable to store window position: {err}");
                });
            })
        });
        window.on("move", move_window.as_ref());
        callbacks.retain(move_window)?;

        let settings_ = settings.clone();
        let resize_window = callback!(move |width: u32, height: u32| {
            let settings_ = settings_.clone();
            let size = Size { width, height };
            log_info!("Size window to ({}, {})", size.width, size.height);
            spawn(async move {
                settings_.set("window.size", size).await.unwrap_or_else(|err| {
                    log_error!("Unable to store window size: {err}");
                });
            })
        });
        window.on("resize", resize_window.as_ref());
        callbacks.retain(resize_window)?;

        if let Some(position) = settings.get::<Position>("window.position").await {
            window.move_to(position.x, position.y);
        }

        if let Some(size) = settings.get::<Size>("window.size").await {
            window.resize_to(size.width, size.height);
        }

        window.show();

        Ok(Self { callbacks, settings: settings.clone(), window: window.clone() })
    }
}
