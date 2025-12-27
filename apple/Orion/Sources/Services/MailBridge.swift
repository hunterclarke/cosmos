import SwiftUI
import Foundation

// MARK: - Type Extensions for SwiftUI compatibility

extension FfiAccount: Identifiable {}
extension FfiThreadSummary: Identifiable {}
extension FfiMessage: Identifiable {}
extension FfiSearchResult: Identifiable {
    public var id: String { threadId }
}

// MARK: - Mail Bridge

/// Swift wrapper around the Rust MailService
///
/// Handles async/await bridging to the synchronous Rust FFI,
/// publishes state changes for SwiftUI views.
@MainActor
class MailBridge: ObservableObject {
    // MARK: - Published State

    @Published var accounts: [FfiAccount] = []
    @Published var threads: [FfiThreadSummary] = []
    @Published var searchResults: [FfiSearchResult] = []
    @Published var currentThreadDetail: FfiThreadDetail? = nil

    @Published var totalCount: UInt32 = 0
    @Published var unreadCount: UInt32 = 0
    @Published var labelUnreadCounts: [String: UInt32] = [:]

    @Published var isLoading: Bool = false
    @Published var isSyncing: Bool = false
    @Published var error: String? = nil
    @Published var syncProgress: SyncProgress? = nil

    @Published var isInitialized: Bool = false

    // MARK: - Private State

    private var service: MailService? = nil
    private let backgroundQueue = DispatchQueue(label: "com.orion.mail", qos: .userInitiated)
    private var lastSyncAt: Date? = nil
    private let syncCooldown: TimeInterval = 30  // 30 seconds minimum between syncs

    /// Whether a sync can be started (respects cooldown)
    var canSync: Bool {
        guard let lastSync = lastSyncAt else { return true }
        return Date().timeIntervalSince(lastSync) >= syncCooldown
    }

    // MARK: - Initialization

    /// Initialize the mail service with platform-appropriate paths
    func initialize() async {
        guard !isInitialized else { return }

        let paths = getDataPaths()

        do {
            // Initialize on background queue since it may do I/O
            let svc = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<MailService, Error>) in
                backgroundQueue.async {
                    do {
                        let mailService = try MailService(
                            dbPath: paths.dbPath,
                            blobPath: paths.blobPath,
                            searchIndexPath: paths.searchPath
                        )
                        continuation.resume(returning: mailService)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }

            self.service = svc
            self.isInitialized = true
            OrionLogger.mailBridge.info("Initialized with db: \(paths.dbPath)")

            // Load initial data
            await loadAccounts()

        } catch {
            self.error = "Failed to initialize mail service: \(error.localizedDescription)"
            OrionLogger.mailBridge.error("Init error: \(error)")
        }
    }

    // MARK: - Account Management

    func loadAccounts() async {
        guard let service = service else { return }

        do {
            let accts = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<[FfiAccount], Error>) in
                backgroundQueue.async {
                    do {
                        let result = try service.listAccounts()
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
            self.accounts = accts
            OrionLogger.mailBridge.info("Loaded \(accts.count) accounts")
        } catch {
            self.error = "Failed to load accounts: \(error.localizedDescription)"
            OrionLogger.mailBridge.error("Load accounts error: \(error)")
        }
    }

    func addAccount(email: String) async -> FfiAccount? {
        guard let service = service else { return nil }

        do {
            let account = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<FfiAccount, Error>) in
                backgroundQueue.async {
                    do {
                        let result = try service.registerAccount(email: email)
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
            await loadAccounts()
            return account
        } catch {
            self.error = "Failed to add account: \(error.localizedDescription)"
            return nil
        }
    }

    // MARK: - Thread Loading

    func loadThreads(label: String?, accountId: Int64?) async {
        guard let service = service else { return }

        isLoading = true
        defer { isLoading = false }

        do {
            let result = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<(threads: [FfiThreadSummary], total: UInt32, unread: UInt32), Error>) in
                backgroundQueue.async {
                    do {
                        let threads = try service.listThreads(
                            label: label,
                            accountId: accountId,
                            limit: 100,
                            offset: 0
                        )
                        let total = try service.countThreads(label: label, accountId: accountId)
                        let unread = try service.countUnread(label: label ?? "INBOX", accountId: accountId)
                        continuation.resume(returning: (threads, total, unread))
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
            self.threads = result.threads
            self.totalCount = result.total
            self.unreadCount = result.unread
            OrionLogger.mailBridge.info("Loaded \(result.threads.count) threads (total: \(result.total), unread: \(result.unread))")
        } catch {
            self.error = "Failed to load threads: \(error.localizedDescription)"
            OrionLogger.mailBridge.error("Load threads error: \(error)")
        }
    }

    func loadThreadDetail(threadId: String) async -> FfiThreadDetail? {
        guard let service = service else { return nil }

        do {
            let detail = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<FfiThreadDetail?, Error>) in
                backgroundQueue.async {
                    do {
                        let result = try service.getThreadDetail(threadId: threadId)
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
            self.currentThreadDetail = detail
            return detail
        } catch {
            self.error = "Failed to load thread detail: \(error.localizedDescription)"
            OrionLogger.mailBridge.error("Load thread detail error: \(error)")
            return nil
        }
    }

    // MARK: - Search

    func search(query: String) async {
        guard let service = service else { return }

        isLoading = true
        defer { isLoading = false }

        do {
            let results = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<[FfiSearchResult], Error>) in
                backgroundQueue.async {
                    do {
                        let result = try service.search(
                            query: query,
                            limit: 50,
                            accountId: nil
                        )
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
            self.searchResults = results
            OrionLogger.mailBridge.info("Search returned \(results.count) results")
        } catch {
            self.error = "Search failed: \(error.localizedDescription)"
            OrionLogger.mailBridge.error("Search error: \(error)")
        }
    }

    // MARK: - Sync

    // Note: Sync requires OAuth tokens which need to come from AuthService
    // For now, this is a placeholder - the GPUI app handles sync

    func syncAccount(accountId: Int64, tokenJson: String, clientId: String, clientSecret: String) async throws -> FfiSyncStats {
        guard let service = service else {
            throw MailError.InvalidArgument(message: "MailService not initialized")
        }

        // Check sync cooldown
        if !canSync {
            OrionLogger.mailBridge.info("Sync skipped - cooldown not elapsed")
            throw MailError.InvalidArgument(message: "Sync cooldown - please wait before syncing again")
        }

        isSyncing = true
        syncProgress = SyncProgress(phase: "Starting sync...", fetched: 0, total: nil)
        OrionLogger.sync.info("Starting sync for account \(accountId)")
        defer {
            isSyncing = false
            syncProgress = nil
        }

        // Track last refresh to throttle during sync (use class for reference semantics)
        let refreshTracker = SyncRefreshTracker()
        let refreshInterval: UInt32 = 100  // Refresh every 100 messages processed

        // Create a callback for progress updates that also refreshes the thread list
        let callback = SwiftSyncProgressCallback { [weak self] fetched, total, phase in
            Task { @MainActor in
                guard let self = self else { return }
                self.syncProgress = SyncProgress(phase: phase, fetched: fetched, total: total)

                // Refresh thread list periodically during "Processing" phase
                // This shows emails as they are ingested
                if phase.contains("Processed") || phase.contains("Processing") {
                    if fetched >= refreshTracker.lastCount + refreshInterval {
                        refreshTracker.lastCount = fetched
                        // Load threads in background to show new emails
                        await self.loadThreads(label: "INBOX", accountId: nil)
                    }
                }
            }
        }

        let stats = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<FfiSyncStats, Error>) in
            backgroundQueue.async {
                do {
                    let result = try service.syncAccount(
                        accountId: accountId,
                        tokenJson: tokenJson,
                        clientId: clientId,
                        clientSecret: clientSecret,
                        callback: callback
                    )
                    continuation.resume(returning: result)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }

        // Update sync timestamp
        lastSyncAt = Date()
        OrionLogger.sync.info("Completed - threads: \(stats.threadsCreated), messages: \(stats.messagesCreated), duration: \(stats.durationMs)ms")

        // Reload data after sync
        await loadAccounts()
        await loadLabelUnreadCounts(accountId: nil)

        return stats
    }

    // MARK: - Actions

    func archiveThread(threadId: String, tokenJson: String, clientId: String, clientSecret: String) async throws {
        guard let service = service else {
            throw MailError.InvalidArgument(message: "MailService not initialized")
        }

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            backgroundQueue.async {
                do {
                    try service.archiveThread(
                        threadId: threadId,
                        tokenJson: tokenJson,
                        clientId: clientId,
                        clientSecret: clientSecret
                    )
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func toggleStar(threadId: String, tokenJson: String, clientId: String, clientSecret: String) async throws -> Bool {
        guard let service = service else {
            throw MailError.InvalidArgument(message: "MailService not initialized")
        }

        return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Bool, Error>) in
            backgroundQueue.async {
                do {
                    let result = try service.toggleStar(
                        threadId: threadId,
                        tokenJson: tokenJson,
                        clientId: clientId,
                        clientSecret: clientSecret
                    )
                    continuation.resume(returning: result)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func setRead(threadId: String, isRead: Bool, tokenJson: String, clientId: String, clientSecret: String) async throws {
        guard let service = service else {
            throw MailError.InvalidArgument(message: "MailService not initialized")
        }

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            backgroundQueue.async {
                do {
                    try service.setRead(
                        threadId: threadId,
                        isRead: isRead,
                        tokenJson: tokenJson,
                        clientId: clientId,
                        clientSecret: clientSecret
                    )
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    func trashThread(threadId: String, tokenJson: String, clientId: String, clientSecret: String) async throws {
        guard let service = service else {
            throw MailError.InvalidArgument(message: "MailService not initialized")
        }

        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            backgroundQueue.async {
                do {
                    try service.trashThread(
                        threadId: threadId,
                        tokenJson: tokenJson,
                        clientId: clientId,
                        clientSecret: clientSecret
                    )
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    // MARK: - Sync State

    /// Get the current sync state for an account
    /// Returns nil if the account has never been synced
    func getSyncState(accountId: Int64) async -> FfiSyncState? {
        guard let service = service else { return nil }

        do {
            return try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<FfiSyncState?, Error>) in
                backgroundQueue.async {
                    do {
                        let result = try service.getSyncState(accountId: accountId)
                        continuation.resume(returning: result)
                    } catch {
                        continuation.resume(throwing: error)
                    }
                }
            }
        } catch {
            OrionLogger.mailBridge.error("Get sync state error: \(error)")
            return nil
        }
    }

    /// Check if any accounts have incomplete syncs that need resuming
    func checkForIncompleteSyncs() async -> [(FfiAccount, FfiSyncState)] {
        var incomplete: [(FfiAccount, FfiSyncState)] = []

        for account in accounts {
            if let state = await getSyncState(accountId: account.id) {
                // If initial sync is not complete, this account needs resuming
                if !state.initialSyncComplete {
                    incomplete.append((account, state))
                }
            }
        }

        return incomplete
    }

    // MARK: - Label Counts

    /// Load unread counts for all labels
    func loadLabelUnreadCounts(accountId: Int64?) async {
        guard let service = service else { return }

        let labels = ["INBOX", "STARRED", "SENT", "DRAFT", "ALL", "SPAM", "TRASH"]
        for label in labels {
            do {
                let count = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<UInt32, Error>) in
                    backgroundQueue.async {
                        do {
                            let result = try service.countUnread(label: label, accountId: accountId)
                            continuation.resume(returning: result)
                        } catch {
                            continuation.resume(throwing: error)
                        }
                    }
                }
                labelUnreadCounts[label] = count
            } catch {
                OrionLogger.mailBridge.error("Count unread error for \(label): \(error)")
            }
        }
    }

    // MARK: - Helpers

    private func getDataPaths() -> (dbPath: String, blobPath: String, searchPath: String) {
        #if os(iOS)
        // iOS: Use Application Support directory within app sandbox
        let appSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!.appendingPathComponent("cosmos")

        try? FileManager.default.createDirectory(at: appSupport, withIntermediateDirectories: true)

        return (
            appSupport.appendingPathComponent("mail.db").path,
            appSupport.appendingPathComponent("mail.blobs").path,
            appSupport.appendingPathComponent("mail.search.idx").path
        )
        #else
        // macOS: Use ~/Library/Application Support/cosmos/ for compatibility with GPUI app
        let appSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!.appendingPathComponent("cosmos")

        try? FileManager.default.createDirectory(at: appSupport, withIntermediateDirectories: true)

        return (
            appSupport.appendingPathComponent("mail.db").path,
            appSupport.appendingPathComponent("mail.blobs").path,
            appSupport.appendingPathComponent("mail.search.idx").path
        )
        #endif
    }
}

// MARK: - Supporting Types

struct SyncProgress {
    let phase: String
    let fetched: UInt32
    let total: UInt32?
}

/// Tracks sync refresh state with reference semantics for closure capture
private class SyncRefreshTracker: @unchecked Sendable {
    var lastCount: UInt32 = 0
}

// MARK: - Sync Progress Callback

final class SwiftSyncProgressCallback: SyncProgressCallback, @unchecked Sendable {
    private let handler: @Sendable (UInt32, UInt32?, String) -> Void

    init(handler: @escaping @Sendable (UInt32, UInt32?, String) -> Void) {
        self.handler = handler
    }

    func onProgress(fetched: UInt32, total: UInt32?, phase: String) {
        if let total = total {
            OrionLogger.sync.info("\(phase): \(fetched)/\(total)")
        } else {
            OrionLogger.sync.info("\(phase): \(fetched) fetched")
        }
        handler(fetched, total, phase)
    }

    func onError(message: String) {
        OrionLogger.sync.error("Error: \(message)")
    }
}
