//! UniFFI bindgen binary for generating Swift/Kotlin bindings
//!
//! Usage:
//!   cargo run -p mail-ffi --bin uniffi-bindgen generate \
//!       --library target/aarch64-apple-darwin/release/libmail_ffi.dylib \
//!       --language swift \
//!       --out-dir generated/swift

fn main() {
    uniffi::uniffi_bindgen_main()
}
