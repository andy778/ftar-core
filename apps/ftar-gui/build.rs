//! Windows gives the main thread 1 MiB of stack, Linux and macOS 8 MiB.
//! Trees up to `ftar_core::model::MAX_DEPTH` levels are walked recursively,
//! so give the Windows binaries the same 8 MiB.

fn main() {
    const STACK: u32 = 8 * 1024 * 1024;
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => println!("cargo:rustc-link-arg-bins=/STACK:{STACK}"),
            Ok("gnu") => println!("cargo:rustc-link-arg-bins=-Wl,--stack,{STACK}"),
            _ => {}
        }
    }
}
