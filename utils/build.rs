use duct::cmd;
use std::env;
use std::path::*;

struct GitHead {
    head_path: String,
    head_ref_path: String,
    full_hash: String,
    short_hash: String,
}

fn main() {
    let success = if env::var("RUSTY_KASPA_NO_COMMIT_HASH").is_err() {
        if let Some(GitHead { head_path, head_ref_path, full_hash, short_hash }) = try_git_head() {
            println!("cargo::rerun-if-changed={head_path}");
            println!("cargo::rerun-if-changed={head_ref_path}");
            println!("cargo:rustc-env=RUSTY_KASPA_GIT_FULL_COMMIT_HASH={full_hash}");
            println!("cargo:rustc-env=RUSTY_KASPA_GIT_SHORT_COMMIT_HASH={short_hash}");
            true
        } else {
            false
        }
    } else {
        false
    };

    if !success {
        println!("cargo:rustc-env=RUSTY_KASPA_GIT_FULL_COMMIT_HASH=");
        println!("cargo:rustc-env=RUSTY_KASPA_GIT_SHORT_COMMIT_HASH=");
    }
}

fn try_git_head() -> Option<GitHead> {
    let cargo_manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let path = cargo_manifest_dir.as_path().parent()?;

    let full_hash = cmd!("git", "rev-parse", "HEAD").dir(path).read().ok().map(|full_hash| full_hash.trim().to_string());

    let short_hash = cmd!("git", "rev-parse", "--short", "HEAD").dir(path).read().ok().map(|short_hash| short_hash.trim().to_string());

    let git_folder = path.join(".git");
    if git_folder.is_dir() {
        let head_path = git_folder.join("HEAD");
        if head_path.is_file() {
            let head = std::fs::read_to_string(&head_path).ok()?;
            if head.starts_with("ref: ") {
                let head_ref_path = head.trim_start_matches("ref: ");
                let head_ref_path = git_folder.join(head_ref_path.trim());
                if head_ref_path.is_file() {
                    if let (Some(full_hash), Some(short_hash)) = (full_hash, short_hash) {
                        return Some(GitHead {
                            head_path: head_path.to_str().unwrap().to_string(),
                            head_ref_path: head_ref_path.to_str().unwrap().to_string(),
                            full_hash,
                            short_hash,
                        });
                    } else if let Ok(full_hash) = std::fs::read_to_string(&head_ref_path) {
                        let full_hash = full_hash.trim().to_string();
                        let short_hash = if full_hash.len() >= 7 {
                            // this is not actually correct as short hash has a variable
                            // length based on commit short hash collisions (which is)
                            // why we attempt to use `git rev-parse` above. But since this
                            // is for reference purposes only, we can live with it.
                            full_hash[0..7].to_string()
                        } else {
                            full_hash.to_string()
                        };

                        return Some(GitHead {
                            head_path: head_path.to_str().unwrap().to_string(),
                            head_ref_path: head_ref_path.to_str().unwrap().to_string(),
                            full_hash,
                            short_hash,
                        });
                    }
                }
            }
        }
    }

    None
}
