//! Puts the icon into shelve.exe, so Explorer shows it.
//! The .ico is made by: cargo run -p shelve-platform --example make_ico

fn main() {
    println!("cargo:rerun-if-changed=assets/shelve.ico");
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("assets/shelve.ico")
        .compile()
        .expect("embedding assets/shelve.ico needs a resource compiler (rc, llvm-rc or windres)");
}
