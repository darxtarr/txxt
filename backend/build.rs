use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let target_dir = out_dir
        .ancestors()
        .nth(3)
        .unwrap();

    fs::copy("settings.json", target_dir.join("settings.json"))
        .expect("Failed to copy settings.json");
}
