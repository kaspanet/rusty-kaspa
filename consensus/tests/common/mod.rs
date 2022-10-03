use std::{
    fs::{self, File, ReadDir},
    io,
    path::Path,
};

#[allow(dead_code)] // Usage by integration tests in ignored by the compiler for some reason
pub fn open_file(file_path: &str) -> File {
    let file_res = File::open(file_path);
    match file_res {
        Ok(file) => file,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                // In debug mode the working directory is often the top-level workspace folder
                let path = Path::new("consensus");
                File::open(path.join(file_path)).unwrap()
            }
            _ => panic!("{}", e),
        },
    }
}

#[allow(dead_code)] // Usage by integration tests in ignored by the compiler
pub fn read_dir(dir_path: &str) -> ReadDir {
    let dir_res = fs::read_dir(dir_path);
    match dir_res {
        Ok(dir) => dir,
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                // In debug mode the working directory is often the top-level workspace folder
                let path = Path::new("consensus");
                fs::read_dir(path.join(dir_path)).unwrap()
            }
            _ => panic!("{}", e),
        },
    }
}
