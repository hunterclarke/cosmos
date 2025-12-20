# Async Architecture in Cosmos

This document explains how async code works in the Cosmos workspace, particularly the interaction between GPUI's runtime and our business logic crates.

## The Problem

GPUI does **not** use Tokio. It has its own custom async executor built on platform-native dispatch mechanisms:

- **macOS**: Grand Central Dispatch (GCD)
- **Linux/Windows**: Platform-specific equivalents

This means any code that depends on Tokio (like `tokio::fs`, `tokio::time::sleep`, or `reqwest` with default features) will panic at runtime with:

```
there is no reactor running, must be called from the context of a Tokio 1.x runtime
```

## The Solution

### Architecture Principle: Executor-Agnostic Business Logic

The `mail` crate is designed to be **executor-agnostic** by using:

1. **Synchronous HTTP** (`ureq`) instead of async HTTP (`reqwest`)
2. **Synchronous file I/O** (`std::fs`) instead of async (`tokio::fs`)
3. **Thread sleep** (`std::thread::sleep`) instead of async (`tokio::time::sleep`)

This makes the mail crate:
- Portable across any async runtime
- Testable without runtime setup
- Ready for UniFFI (mobile) where Tokio may not be available

### UI Layer Integration

The Orion app integrates with the sync business logic using GPUI's executor:

```rust
// In app.rs
pub fn sync(&mut self, cx: &mut Context<Self>) {
    let client = self.gmail_client.clone();
    let store = self.store.clone();

    // Get handle to background executor
    let background = cx.background_executor().clone();

    // Spawn on foreground (for UI updates)
    cx.spawn(async move |this, cx| {
        // Run blocking work on background thread pool
        let result = background
            .spawn(async move {
                sync_inbox(&client, store.as_ref(), 100)  // Sync function!
            })
            .await;

        // Update UI on main thread
        cx.update(|cx| {
            this.update(cx, |app, cx| {
                // Handle result, update state
                cx.notify();  // Trigger re-render
            })
        })
    })
    .detach();
}
```

## GPUI Executors

GPUI provides two executors:

### ForegroundExecutor
- Runs on the main thread
- Used for UI updates and quick operations
- Access via `cx.spawn()`

### BackgroundExecutor
- Runs on a thread pool
- Used for blocking I/O, heavy computation
- Access via `cx.background_executor().spawn()`

## Key Patterns

### 1. Background Work with UI Updates

```rust
let background = cx.background_executor().clone();
cx.spawn(async move |this, cx| {
    // Do blocking work on background thread
    let result = background.spawn(async move {
        expensive_sync_operation()
    }).await;

    // Update UI on main thread
    cx.update(|cx| {
        this.update(cx, |app, cx| {
            app.data = result;
            cx.notify();
        })
    })
}).detach();
```

### 2. Fire and Forget

```rust
cx.spawn(async move |_, _| {
    // Work that doesn't need to update UI
}).detach();
```

### 3. Storing Task Handle

```rust
struct MyView {
    task: Option<Task<()>>,
}

impl MyView {
    fn start_work(&mut self, cx: &mut Context<Self>) {
        self.task = Some(cx.spawn(async move |_, _| {
            // Task cancelled if MyView is dropped
        }));
    }
}
```

## Why Not Tokio Compatibility?

We explicitly chose sync I/O over async for several reasons:

1. **Simplicity**: No runtime conflicts, works everywhere
2. **Portability**: UniFFI for iOS/Android doesn't play well with Tokio
3. **Testing**: Unit tests don't need runtime setup
4. **GPUI Integration**: Clean separation between UI async and business sync

The mail crate performs HTTP requests (Gmail API) and file I/O (token storage). Both are fundamentally blocking operations - wrapping them in async just adds complexity without real benefit for our use case.

## Dependencies

### Allowed in mail crate:
- `ureq` - Sync HTTP client
- `std::fs` - Sync file I/O
- `std::thread::sleep` - Sync delays
- `serde`, `chrono`, `anyhow` - General utilities

### NOT allowed in mail crate:
- `tokio` - Would conflict with GPUI
- `reqwest` (default features) - Uses Tokio internally
- `async-std` - Another runtime
- Any `gpui` imports - Keep business logic UI-free

## Testing

Business logic tests run without any async runtime:

```rust
#[test]
fn test_sync_operation() {
    let store = InMemoryMailStore::new();
    // Direct sync calls - no runtime needed
    store.upsert_thread(thread).unwrap();
    let threads = store.list_threads(10, 0).unwrap();
    assert_eq!(threads.len(), 1);
}
```

## References

- [Zed Decoded: Async Rust](https://zed.dev/blog/zed-decoded-async-rust)
- [GPUI Documentation](https://docs.rs/gpui/latest/gpui/)
- [GPUI Contexts Guide](https://github.com/zed-industries/zed/blob/main/crates/gpui/docs/contexts.md)
