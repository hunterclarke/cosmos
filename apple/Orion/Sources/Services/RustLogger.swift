import Foundation
import os.log

/// Swift implementation of the Rust LogCallback protocol
///
/// Routes Rust log messages to Apple's unified logging system (os_log/Logger).
/// Uses the Logger API (iOS 14+, macOS 11+) for structured logging.
final class RustLogCallback: LogCallback, @unchecked Sendable {
    /// Logger instance using Cosmos subsystem
    private let logger = Logger(subsystem: "com.cosmos.orion", category: "Rust")

    func onLog(level: FfiLogLevel, target: String, message: String) {
        // Format: [target] message
        let formattedMessage = "[\(target)] \(message)"

        switch level {
        case .error:
            logger.error("\(formattedMessage, privacy: .public)")
        case .warn:
            logger.warning("\(formattedMessage, privacy: .public)")
        case .info:
            logger.info("\(formattedMessage, privacy: .public)")
        case .debug:
            logger.debug("\(formattedMessage, privacy: .public)")
        case .trace:
            logger.trace("\(formattedMessage, privacy: .public)")
        }
    }
}

/// Initialize Rust logging at app startup
///
/// This connects Rust's log crate to Apple's unified logging via our callback.
/// Should be called once at app startup, before any Rust code runs.
///
/// - Parameter debug: If true, enables debug-level logging; otherwise info-level.
func initializeRustLogging(debug: Bool = false) {
    let maxLevel: UInt8 = debug ? 3 : 2 // 2=info, 3=debug
    let callback = RustLogCallback()
    initializeLogging(callback: callback, maxLevel: maxLevel)
}
