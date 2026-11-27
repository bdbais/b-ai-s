// Passa lo script del linker (kernel caricato da GRUB a 1 MiB) e le opzioni per un binario senza libc.
fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{}/link.ld", dir);
    for a in ["-nostartfiles", "-nostdlib", "-static", "-no-pie", "-Wl,--build-id=none", "-Wl,--gc-sections", "-Wl,-z,max-page-size=4096"] {
        println!("cargo:rustc-link-arg-bins={}", a);
    }
    println!("cargo:rerun-if-changed=link.ld");
    println!("cargo:rerun-if-changed=src/boot.s");
}
