use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=macos/Info.plist");

    let target = env::var("TARGET").unwrap_or_default();
    if !target.ends_with("apple-darwin") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let plist = manifest_dir.join("macos").join("Info.plist");

    println!(
        "cargo:rustc-link-arg-bin=late=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
}
