// Passa lo script del linker (indirizzo di caricamento 0x80000 del firmware del Raspberry).
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", dir);
    println!("cargo:rerun-if-changed=link.ld");
}
