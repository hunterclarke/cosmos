import Foundation
import os.log

/// Centralized logging for the Orion app
///
/// Provides a consistent logging interface using Apple's unified logging system.
/// Use the static loggers for common categories or create instances for specific ones.
struct OrionLogger {
    private let logger: Logger

    /// Create a logger for a specific category
    init(category: String) {
        self.logger = Logger(subsystem: "com.cosmos.orion", category: category)
    }

    // MARK: - Static Loggers for Common Categories

    static let app = OrionLogger(category: "App")
    static let mailBridge = OrionLogger(category: "MailBridge")
    static let auth = OrionLogger(category: "Auth")
    static let sync = OrionLogger(category: "Sync")
    static let ui = OrionLogger(category: "UI")

    // MARK: - Logging Methods

    func error(_ message: String) {
        logger.error("\(message, privacy: .public)")
    }

    func warning(_ message: String) {
        logger.warning("\(message, privacy: .public)")
    }

    func info(_ message: String) {
        logger.info("\(message, privacy: .public)")
    }

    func debug(_ message: String) {
        logger.debug("\(message, privacy: .public)")
    }

    func trace(_ message: String) {
        logger.trace("\(message, privacy: .public)")
    }
}
