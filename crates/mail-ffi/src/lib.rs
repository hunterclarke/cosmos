//! UniFFI bindings crate for the mail library
//!
//! This crate wraps the mail crate for UniFFI library mode binding generation.
//! It re-exports the FFI module and UniFFI scaffolding from the mail crate.
//!
//! ## Building for Swift
//!
//! 1. Build the library for Apple platforms:
//!    ```bash
//!    cargo build --release -p mail-ffi --target aarch64-apple-darwin
//!    cargo build --release -p mail-ffi --target aarch64-apple-ios
//!    ```
//!
//! 2. Generate Swift bindings:
//!    ```bash
//!    cargo run -p mail-ffi --features bindgen --bin uniffi-bindgen generate \
//!        --library target/aarch64-apple-darwin/release/libmail_ffi.dylib \
//!        --language swift \
//!        --out-dir generated/swift
//!    ```
//!
//! 3. Create XCFramework (see script/build-xcframework)

// Re-export everything from the mail crate's FFI module
pub use mail::ffi::*;

// Re-export the uniffi scaffolding from the mail crate
// This is needed for library mode to work correctly
mail::uniffi_reexport_scaffolding!();
