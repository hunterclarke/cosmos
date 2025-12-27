# Development & Release Workflows

This document describes the development and release workflows for Orion apps.

## Development Setup

### Prerequisites

1. **Rust toolchain** - Install via [rustup](https://rustup.rs/)
2. **Xcode** - Required for SwiftUI builds (macOS/iOS)
3. **OAuth credentials** - Get from [Google Cloud Console](https://console.cloud.google.com)

### One-Time Setup

```bash
# 1. Clone the repository
git clone https://github.com/your-org/cosmos.git
cd cosmos

# 2. Install cross-compilation targets (for SwiftUI XCFramework)
rustup target add x86_64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

# 3. Set up OAuth credentials
cp secrets/google-credentials.json.template secrets/google-credentials.json
# Edit the file with your OAuth credentials from Google Cloud Console

# 4. Configure credentials for both apps
./script/setup-credentials
```

## Development Workflow

### GPUI App (macOS/Linux/Windows)

```bash
# Run in debug mode
cargo run -p orion

# Run in release mode (faster)
cargo run -p orion --release

# Or use the script
./script/run-gpui
./script/run-gpui --release
```

Credentials are loaded from `~/Library/Application Support/cosmos/google-credentials.json` (symlink created by setup script).

### SwiftUI App (macOS/iOS)

```bash
# Build XCFramework first (required after Rust changes)
./script/build-xcframework

# Open in Xcode
./script/run-macos    # macOS
./script/run-ios      # iOS Simulator
```

Credentials are embedded at build time via xcconfig files.

### After Rust Changes

If you modify the `mail` or `mail-ffi` crates:

```bash
# Rebuild XCFramework for SwiftUI
./script/build-xcframework
```

## Release Workflow

### GPUI App Release

For production builds, OAuth credentials are embedded at compile time so the final binary is self-contained:

```bash
# Option 1: Use the convenience script (recommended)
./script/build-gpui-release

# Option 2: Manual build with environment variables
GOOGLE_CLIENT_ID='your-client-id.apps.googleusercontent.com' \
GOOGLE_CLIENT_SECRET='your-secret' \
cargo build -p orion --release
```

The output binary at `target/release/orion` has credentials baked in and requires no external configuration files.

### SwiftUI App Release

Credentials are automatically embedded via xcconfig when building in Xcode:

1. Open `apple/Orion/Orion.xcodeproj` in Xcode
2. Select "Release" configuration
3. Build → Archive

The archived app has credentials embedded from `apple/Orion/Config/Secrets.xcconfig`.

### XCFramework Release

To create an XCFramework for distribution:

```bash
./script/build-xcframework

# Output: generated/MailFFI.xcframework
# Swift bindings: generated/mail_ffi.swift
```

## Credential Management

### Credential Files

| File | Purpose | Committed |
|------|---------|-----------|
| `secrets/google-credentials.json` | Single source of truth | No (gitignored) |
| `secrets/google-credentials.json.template` | Template for new setups | Yes |
| `apple/Orion/Config/Secrets.xcconfig` | Generated for SwiftUI | No (gitignored) |

### How Credentials Are Loaded

**Development:**
- GPUI: Reads from `~/Library/Application Support/cosmos/google-credentials.json` (symlink)
- SwiftUI: Embedded via xcconfig → Info.plist → Bundle at build time

**Production:**
- GPUI: Embedded at compile time via `option_env!()` macro
- SwiftUI: Same as development (xcconfig embedding)

### Updating Credentials

```bash
# 1. Update the source file
vim secrets/google-credentials.json

# 2. Regenerate config files
./script/setup-credentials
```

## Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p mail
cargo test -p orion

# Run with output
cargo test -- --nocapture
```

## Troubleshooting

### OAuth not working

1. Verify credentials are configured:
   ```bash
   cat secrets/google-credentials.json
   ```

2. Re-run setup:
   ```bash
   ./script/setup-credentials
   ```

3. Check symlink (GPUI):
   ```bash
   ls -la ~/Library/Application\ Support/cosmos/google-credentials.json
   ```

### SwiftUI build fails

1. Rebuild XCFramework:
   ```bash
   ./script/build-xcframework
   ```

2. Clean Xcode derived data:
   ```bash
   rm -rf ~/Library/Developer/Xcode/DerivedData/Orion-*
   ```

### XCFramework targets

The XCFramework includes:
- macOS: arm64 (Apple Silicon), x86_64 (Intel)
- iOS: arm64 (devices)
- iOS Simulator: arm64 (Apple Silicon), x86_64 (Intel)

Deployment targets: macOS 26.0, iOS 26.0
