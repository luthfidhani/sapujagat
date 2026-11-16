fn main() {
    // re-embed the executable icon whenever the icon files change
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
