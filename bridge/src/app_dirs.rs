use std::path::PathBuf;

const BRIDGE_APP_FOLDER_NAME: &str = "kaspa-stratum-bridge";

fn get_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return dirs::data_local_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")); // Fallback to current directory

    #[cfg(not(target_os = "windows"))]
    return dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")); // Fallback to current directory
}

pub(crate) fn get_bridge_app_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return get_home_dir().join(BRIDGE_APP_FOLDER_NAME);
    #[cfg(not(target_os = "windows"))]
    return get_home_dir().join(format!(".{}", BRIDGE_APP_FOLDER_NAME));
}

pub(crate) fn get_bridge_logs_dir() -> PathBuf {
    get_bridge_app_dir().join("logs")
}

pub(crate) fn default_inprocess_kaspad_appdir() -> PathBuf {
    get_bridge_app_dir().join("kaspad")
}
