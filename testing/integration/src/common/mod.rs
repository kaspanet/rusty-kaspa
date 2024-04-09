use std::{
    fs::{self, File, ReadDir},
    io,
    path::Path,
};

pub mod args;
pub mod client;
pub mod client_notify;
pub mod daemon;
pub mod listener;
pub mod utils;

pub fn open_file(file_path: &Path) -> File {
    let file_res = File::open(file_path);
    match file_res {
        Ok(file) => file,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                // In debug mode the working directory is often the top-level workspace folder
                let path = Path::new("testing/integration");
                File::open(path.join(file_path)).unwrap()
            }
            _ => panic!("{}", e),
        },
    }
}

pub fn file_exists(file_path: &Path) -> bool {
    if !file_path.exists() {
        // In debug mode the working directory is often the top-level workspace folder
        return Path::new("testing/integration").join(file_path).exists();
    }
    true
}

pub fn read_dir(dir_path: &str) -> ReadDir {
    let dir_res = fs::read_dir(dir_path);
    match dir_res {
        Ok(dir) => dir,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                // In debug mode the working directory is often the top-level workspace folder
                let path = Path::new("testing/integration");
                fs::read_dir(path.join(dir_path)).unwrap()
            }
            _ => panic!("{}", e),
        },
    }
}
