import SwiftUI

/// Main content view with sidebar + content layout
/// Adapts to compact (iPhone) vs regular (iPad/macOS) size classes
struct ContentView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService

    // Size class for responsive layout
    @Environment(\.horizontalSizeClass) var horizontalSizeClass

    @State private var selectedLabel: String? = "INBOX"
    @State private var selectedAccountId: Int64? = nil
    @State private var selectedThread: FfiThreadSummary? = nil
    @State private var searchQuery: String = ""
    @State private var isSearching: Bool = false
    @State private var isSearchEditing: Bool = false
    @State private var showShortcutsHelp: Bool = false
    @State private var showingError: Bool = false
    @State private var errorMessage: String = ""

    // iPhone navigation path
    @State private var iPhoneNavigationPath = NavigationPath()

    // Keyboard navigation state
    @State private var selectedThreadIndex: Int = 0
    @State private var pendingGSequence: Bool = false
    @FocusState private var isContentFocused: Bool

    // Background sync polling
    @State private var pollTask: Task<Void, Never>? = nil

    // Standard Gmail labels
    static let labels: [(id: String, name: String, icon: String)] = [
        ("INBOX", "Inbox", "tray"),
        ("STARRED", "Starred", "star"),
        ("SENT", "Sent", "paperplane"),
        ("DRAFT", "Drafts", "doc"),
        ("ALL", "All Mail", "folder"),
        ("SPAM", "Spam", "exclamationmark.shield"),
        ("TRASH", "Trash", "trash")
    ]


    var body: some View {
        Group {
            #if os(iOS)
            if horizontalSizeClass == .compact {
                iPhoneLayout
            } else {
                iPadMacLayout
            }
            #else
            iPadMacLayout
            #endif
        }
        .onAppear {
            // Focus the main content area, not the search box
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
                isContentFocused = true
            }
            Task {
                await mailBridge.initialize()

                // Resume any incomplete syncs from previous sessions
                await resumeIncompleteSyncs()
            }
            // Start background sync polling
            startPolling()
        }
        .onDisappear {
            // Cancel polling when view disappears
            pollTask?.cancel()
            pollTask = nil
        }
        .onReceive(NotificationCenter.default.publisher(for: .showKeyboardShortcuts)) { _ in
            showShortcutsHelp.toggle()
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            isSearching = true
        }
        // Reset selected index when threads change
        .onChange(of: mailBridge.threads) { _, _ in
            selectedThreadIndex = 0
        }
        .alert("Error", isPresented: $showingError) {
            Button("OK", role: .cancel) { }
        } message: {
            Text(errorMessage)
        }
    }

    // MARK: - iPhone Layout (Compact)

    #if os(iOS)
    private var iPhoneLayout: some View {
        NavigationStack(path: $iPhoneNavigationPath) {
            // Root: Labels screen with account picker
            iPhoneLabelsScreen
                .navigationDestination(for: String.self) { labelId in
                    // Thread list for selected label
                    iPhoneThreadListScreen(labelId: labelId)
                }
        }
        .onAppear {
            // Auto-navigate to Inbox on launch
            if iPhoneNavigationPath.isEmpty {
                iPhoneNavigationPath.append(selectedLabel ?? "INBOX")
            }
        }
    }

    /// Labels screen - root of iPhone navigation
    /// Shows account picker at top, then list of labels
    private var iPhoneLabelsScreen: some View {
        List {
            // Account Picker Section
            Section {
                Menu {
                    // All Accounts option
                    Button {
                        selectedAccountId = nil
                    } label: {
                        if selectedAccountId == nil {
                            Label("All Accounts", systemImage: "checkmark")
                        } else {
                            Text("All Accounts")
                        }
                    }

                    Divider()

                    // Individual accounts
                    ForEach(mailBridge.accounts) { account in
                        Button {
                            selectedAccountId = account.id
                        } label: {
                            if selectedAccountId == account.id {
                                Label(account.displayName ?? account.email, systemImage: "checkmark")
                            } else {
                                Text(account.displayName ?? account.email)
                            }
                        }
                    }

                    Divider()

                    // Add account
                    Button {
                        addAccount()
                    } label: {
                        Label("Add Account", systemImage: "plus")
                    }
                    .disabled(!authService.isConfigured || !mailBridge.isInitialized)
                } label: {
                    HStack {
                        // Account avatar
                        if let accountId = selectedAccountId,
                           let account = mailBridge.accounts.first(where: { $0.id == accountId }) {
                            Circle()
                                .fill(Color(hex: account.avatarColor))
                                .frame(width: 36, height: 36)
                                .overlay(
                                    Text(String(account.email.prefix(1)).uppercased())
                                        .font(.system(size: 16, weight: .medium))
                                        .foregroundColor(.white)
                                )
                            VStack(alignment: .leading, spacing: 2) {
                                Text(account.displayName ?? account.email)
                                    .font(.system(size: 17, weight: .semibold))
                                    .foregroundColor(OrionTheme.foreground)
                                Text(account.email)
                                    .font(.system(size: 13))
                                    .foregroundColor(OrionTheme.mutedForeground)
                            }
                        } else {
                            Circle()
                                .fill(OrionTheme.primary)
                                .frame(width: 36, height: 36)
                                .overlay(
                                    Image(systemName: "tray.2")
                                        .font(.system(size: 16))
                                        .foregroundColor(.white)
                                )
                            Text("All Accounts")
                                .font(.system(size: 17, weight: .semibold))
                                .foregroundColor(OrionTheme.foreground)
                        }

                        Spacer()

                        Image(systemName: "chevron.up.chevron.down")
                            .font(.system(size: 12))
                            .foregroundColor(OrionTheme.mutedForeground)
                    }
                    .padding(.vertical, 4)
                }
            }

            // Labels Section
            Section("Mailboxes") {
                ForEach(Self.labels, id: \.id) { label in
                    Button {
                        selectedLabel = label.id
                        iPhoneNavigationPath.append(label.id)
                    } label: {
                        HStack {
                            Image(systemName: label.icon)
                                .font(.system(size: 18))
                                .foregroundColor(OrionTheme.primary)
                                .frame(width: 28)

                            Text(label.name)
                                .font(.system(size: 17))
                                .foregroundColor(OrionTheme.foreground)

                            Spacer()

                            // Unread count
                            if let count = mailBridge.labelUnreadCounts[label.id], count > 0 {
                                Text("\(count)")
                                    .font(.system(size: 15))
                                    .foregroundColor(OrionTheme.mutedForeground)
                            }

                            Image(systemName: "chevron.right")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundColor(OrionTheme.mutedForeground.opacity(0.5))
                        }
                    }
                }
            }

            // Actions Section
            Section {
                if mailBridge.isSyncing {
                    // Show non-interactive sync status
                    HStack {
                        ProgressView()
                            .scaleEffect(0.8)
                        Text("Syncing...")
                            .foregroundColor(OrionTheme.mutedForeground)
                    }
                } else {
                    Button {
                        syncAllAccounts()
                    } label: {
                        HStack {
                            Image(systemName: "arrow.clockwise")
                                .foregroundColor(OrionTheme.primary)
                            Text("Sync All Accounts")
                                .foregroundColor(OrionTheme.foreground)
                        }
                    }
                    .disabled(!authService.isConfigured || mailBridge.accounts.isEmpty || !mailBridge.isInitialized)
                }
            }

            // Settings Section
            Section {
                NavigationLink {
                    SettingsView()
                } label: {
                    HStack {
                        Image(systemName: "gear")
                            .foregroundColor(OrionTheme.mutedForeground)
                            .frame(width: 28)
                        Text("Settings")
                            .foregroundColor(OrionTheme.foreground)
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Mailboxes")
        .onAppear {
            OrionLogger.ui.info("Mailboxes: \(mailBridge.accounts.count) accounts, selectedAccountId=\(selectedAccountId.map(String.init) ?? "nil")")
            for account in mailBridge.accounts {
                OrionLogger.ui.info("  Account \(account.id): \(account.email)")
            }
        }
    }

    /// Thread list screen for iPhone - pushed from labels
    private func iPhoneThreadListScreen(labelId: String) -> some View {
        ZStack {
            if isSearching {
                SearchResultsView(
                    query: searchQuery,
                    onSelectThread: { result in
                        navigateToSearchResult(result)
                    }
                )
            } else {
                ThreadListView(
                    label: labelId,
                    accountId: selectedAccountId,
                    selectedIndex: $selectedThreadIndex,
                    onSelectThread: { thread in
                        selectedThread = thread
                    },
                    onArchive: archiveThread,
                    onStar: starThread,
                    onToggleRead: toggleReadThread
                )
                .id("\(labelId)-\(selectedAccountId.map(String.init) ?? "all")")
                .onAppear {
                    OrionLogger.ui.info("ThreadListView: label=\(labelId), accountId=\(selectedAccountId.map(String.init) ?? "nil"), threads=\(mailBridge.threads.count)")
                }
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchQuery, isPresented: $isSearching, prompt: "Search emails")
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 8) {
                    Text(isSearching ? "Search" : labelName(for: labelId))
                        .font(.headline)
                    if mailBridge.isSyncing {
                        ProgressView()
                            .scaleEffect(0.7)
                    }
                }
            }
        }
        .navigationDestination(item: $selectedThread) { thread in
            ThreadDetailView(
                thread: thread,
                onBack: { selectedThread = nil },
                onArchive: { archiveThread(thread) },
                onStar: { starThread(thread) },
                onToggleRead: { toggleReadThread(thread) }
            )
        }
    }

    private func labelName(for labelId: String) -> String {
        Self.labels.first(where: { $0.id == labelId })?.name ?? labelId
    }
    #endif

    // MARK: - iPad/macOS Layout (Regular)

    private var iPadMacLayout: some View {
        NavigationSplitView {
            SidebarView(
                selectedLabel: $selectedLabel,
                selectedAccountId: $selectedAccountId
            )
            #if os(macOS)
            .frame(width: 240)
            #else
            .navigationSplitViewColumnWidth(min: 200, ideal: 260, max: 320)
            #endif
        } detail: {
            VStack(spacing: 0) {
                // Top bar with title and search
                HStack {
                    HStack(spacing: OrionTheme.spacing2) {
                        Text(currentLabelTitle)
                            .font(.system(size: OrionTheme.textLg, weight: .semibold))
                            .foregroundColor(OrionTheme.foreground)

                        // Subtle sync indicator
                        if mailBridge.isSyncing {
                            ProgressView()
                                .scaleEffect(0.7)
                                .help("Syncing...")
                        }
                    }

                    Spacer()

                    SearchBox(
                        query: $searchQuery,
                        isSearching: $isSearching,
                        isEditing: $isSearchEditing
                    )
                }
                .padding(.horizontal, OrionTheme.spacing3)
                .padding(.vertical, OrionTheme.spacing2)
                .background(OrionTheme.background)

                // Main content
                ZStack {
                    // Keep ThreadListView alive to avoid reload on navigation back
                    ThreadListView(
                        label: selectedLabel,
                        accountId: selectedAccountId,
                        selectedIndex: $selectedThreadIndex,
                        onSelectThread: { thread in
                            selectedThread = thread
                        }
                    )
                    .opacity(selectedThread == nil && !isSearching ? 1 : 0)
                    .allowsHitTesting(selectedThread == nil && !isSearching)

                    if isSearching && !searchQuery.isEmpty {
                        SearchResultsView(
                            query: searchQuery,
                            onSelectThread: { result in
                                navigateToSearchResult(result)
                            }
                        )
                    }

                    if let thread = selectedThread {
                        ThreadDetailView(
                            thread: thread,
                            onBack: { selectedThread = nil }
                        )
                    }

                    // Shortcuts help modal
                    if showShortcutsHelp {
                        ShortcutsHelpView(isPresented: $showShortcutsHelp)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(OrionTheme.background)
        }
        .navigationSplitViewStyle(.balanced)
        .focusable()
        .focusEffectDisabled()
        .focused($isContentFocused)
        .onKeyPress { keyPress in
            handleKeyPress(keyPress)
        }
    }

    // MARK: - Keyboard Handling

    private func handleKeyPress(_ keyPress: KeyPress) -> KeyPress.Result {
        // Don't handle keys when showing help (except Escape)
        if showShortcutsHelp {
            if keyPress.key == .escape {
                showShortcutsHelp = false
                return .handled
            }
            return .ignored
        }

        // When actively editing search, only handle Escape
        if isSearchEditing {
            if keyPress.key == .escape {
                handleEscape()
                return .handled
            }
            return .ignored
        }

        // Handle G-sequence second key
        if pendingGSequence {
            pendingGSequence = false
            return handleGSequence(keyPress)
        }

        // Handle regular keys (only when not searching)
        switch keyPress.key {
        case .init("g") where keyPress.modifiers.isEmpty:
            pendingGSequence = true
            return .handled

        case .init("j"), .downArrow:
            moveDown()
            return .handled

        case .init("k"), .upArrow:
            moveUp()
            return .handled

        case .return:
            openSelected()
            return .handled

        case .escape:
            handleEscape()
            return .handled

        case .init("/"):
            NotificationCenter.default.post(name: .focusSearch, object: nil)
            return .handled

        case .init("?"):
            showShortcutsHelp = true
            return .handled

        case .init("e"):
            archiveSelected()
            return .handled

        case .init("s"):
            starSelected()
            return .handled

        case .init("u"):
            toggleReadSelected()
            return .handled

        case .init("#"):
            trashSelected()
            return .handled

        case .init("3") where keyPress.modifiers.contains(.shift):
            trashSelected()
            return .handled

        default:
            return .ignored
        }
    }

    private func handleGSequence(_ keyPress: KeyPress) -> KeyPress.Result {
        switch keyPress.key {
        case .init("i"):
            selectedLabel = "INBOX"
            return .handled
        case .init("s"):
            selectedLabel = "STARRED"
            return .handled
        case .init("t"):
            selectedLabel = "SENT"
            return .handled
        case .init("d"):
            selectedLabel = "DRAFT"
            return .handled
        case .init("a"):
            selectedLabel = "ALL"
            return .handled
        case .init("#"), .init("3"):
            selectedLabel = "TRASH"
            return .handled
        default:
            return .ignored
        }
    }

    // MARK: - Navigation Actions

    private func moveDown() {
        guard selectedThread == nil else { return }
        let maxIndex = mailBridge.threads.count - 1
        if selectedThreadIndex < maxIndex {
            selectedThreadIndex += 1
        }
    }

    private func moveUp() {
        guard selectedThread == nil else { return }
        if selectedThreadIndex > 0 {
            selectedThreadIndex -= 1
        }
    }

    private func openSelected() {
        guard selectedThread == nil else { return }
        guard selectedThreadIndex < mailBridge.threads.count else { return }
        selectedThread = mailBridge.threads[selectedThreadIndex]
    }

    private func handleEscape() {
        if selectedThread != nil {
            selectedThread = nil
        } else if isSearchEditing {
            isSearchEditing = false
        } else if isSearching {
            isSearching = false
            isSearchEditing = false
            searchQuery = ""
        }
    }

    /// Navigate to a thread from search results
    /// Preserves search state so user can go back to results
    private func navigateToSearchResult(_ result: FfiSearchResult) {
        Task {
            // Load the full thread detail to get account_id and other info
            if let detail = await mailBridge.loadThreadDetail(threadId: result.threadId) {
                // Convert to summary for navigation
                // Don't clear search state - preserve it for back navigation
                selectedThread = detail.thread.toSummary()
            }
        }
    }

    // MARK: - Thread Actions

    private func archiveSelected() {
        guard let thread = selectedThread ?? currentThread else { return }
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                try await mailBridge.archiveThread(
                    threadId: thread.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                // Move to next thread or go back to list
                if selectedThread != nil {
                    selectedThread = nil
                }
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Archive failed: \(error)")
            }
        }
    }

    private func starSelected() {
        guard let thread = selectedThread ?? currentThread else { return }
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                _ = try await mailBridge.toggleStar(
                    threadId: thread.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Star failed: \(error)")
            }
        }
    }

    private func toggleReadSelected() {
        guard let thread = selectedThread ?? currentThread else { return }
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                try await mailBridge.setRead(
                    threadId: thread.id,
                    isRead: thread.isUnread, // Toggle: if unread, mark as read
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Toggle read failed: \(error)")
            }
        }
    }

    private func trashSelected() {
        guard let thread = selectedThread ?? currentThread else { return }
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                try await mailBridge.trashThread(
                    threadId: thread.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                // Move to next thread or go back to list
                if selectedThread != nil {
                    selectedThread = nil
                }
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Trash failed: \(error)")
            }
        }
    }

    // MARK: - Background Sync Polling

    private func startPolling() {
        pollTask = Task {
            while !Task.isCancelled {
                // Wait 60 seconds between syncs
                try? await Task.sleep(for: .seconds(60))

                // Skip if cancelled or already syncing
                guard !Task.isCancelled, !mailBridge.isSyncing else { continue }

                // Check cooldown (MailBridge enforces this too, but avoid unnecessary work)
                guard mailBridge.canSync else { continue }

                // Sync all accounts
                await syncAllAccountsInBackground()
            }
        }
    }

    private func syncAllAccountsInBackground() async {
        for account in mailBridge.accounts {
            do {
                let tokens = try await authService.getValidTokens(for: account.id)
                let _ = try await mailBridge.syncAccount(
                    accountId: account.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
            } catch {
                // Silently log errors during background sync
                OrionLogger.sync.error("Background sync failed for \(account.email): \(error)")
            }
        }
        // Refresh thread list after background sync
        await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
    }

    /// Resume any incomplete syncs from previous sessions
    private func resumeIncompleteSyncs() async {
        guard authService.isConfigured else { return }

        let incompleteSyncs = await mailBridge.checkForIncompleteSyncs()

        if !incompleteSyncs.isEmpty {
            OrionLogger.sync.info("Found \(incompleteSyncs.count) incomplete sync(s) to resume")
        }

        for (account, syncState) in incompleteSyncs {
            OrionLogger.sync.info("Resuming sync for \(account.email) - last sync at \(syncState.lastSyncAt)")

            // Get tokens for this account
            do {
                let tokens = try await authService.getValidTokens(for: account.id)

                // Start sync in background (don't block)
                Task.detached { [mailBridge, authService] in
                    do {
                        let _ = try await mailBridge.syncAccount(
                            accountId: account.id,
                            tokenJson: tokens.toTokenJson(),
                            clientId: authService.clientId,
                            clientSecret: authService.clientSecret
                        )
                    } catch {
                        await MainActor.run {
                            OrionLogger.sync.error("Resume sync failed for \(account.email): \(error)")
                        }
                    }
                }
            } catch {
                OrionLogger.sync.error("Failed to get tokens for \(account.email): \(error)")
            }
        }
    }

    // MARK: - Thread Actions (for swipe gestures)

    private func archiveThread(_ thread: FfiThreadSummary) {
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                try await mailBridge.archiveThread(
                    threadId: thread.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Archive failed: \(error)")
            }
        }
    }

    private func starThread(_ thread: FfiThreadSummary) {
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                _ = try await mailBridge.toggleStar(
                    threadId: thread.id,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Star failed: \(error)")
            }
        }
    }

    private func toggleReadThread(_ thread: FfiThreadSummary) {
        Task {
            do {
                let tokens = try await authService.getValidTokens(for: thread.accountId)
                try await mailBridge.setRead(
                    threadId: thread.id,
                    isRead: thread.isUnread,
                    tokenJson: tokens.toTokenJson(),
                    clientId: authService.clientId,
                    clientSecret: authService.clientSecret
                )
                await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
            } catch {
                OrionLogger.ui.error("Toggle read failed: \(error)")
            }
        }
    }

    // MARK: - Account Actions (for iPhone layout)

    private func addAccount() {
        guard authService.isConfigured, mailBridge.isInitialized else { return }

        Task {
            do {
                let tokens = try await authService.authenticate()
                let email = try await fetchUserEmail(accessToken: tokens.accessToken)

                if let account = await mailBridge.addAccount(email: email) {
                    try authService.saveTokens(tokens, for: account.id)
                    OrionLogger.ui.info("Added account: \(email)")

                    // Load threads immediately (will be empty but ready)
                    await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)

                    // Start sync in background - don't await it
                    // Progress callback will refresh threads as emails arrive
                    let tokenJson = tokens.toTokenJson()
                    Task.detached { [mailBridge, authService] in
                        do {
                            let _ = try await mailBridge.syncAccount(
                                accountId: account.id,
                                tokenJson: tokenJson,
                                clientId: authService.clientId,
                                clientSecret: authService.clientSecret
                            )
                        } catch {
                            await MainActor.run {
                                OrionLogger.sync.error("Initial sync failed for \(email): \(error)")
                            }
                        }
                    }
                } else {
                    errorMessage = "Failed to register account"
                    showingError = true
                }
            } catch {
                errorMessage = error.localizedDescription
                showingError = true
                OrionLogger.ui.error("Add account failed: \(error)")
            }
        }
    }

    private func syncAllAccounts() {
        Task {
            for account in mailBridge.accounts {
                do {
                    let tokens = try await authService.getValidTokens(for: account.id)
                    let _ = try await mailBridge.syncAccount(
                        accountId: account.id,
                        tokenJson: tokens.toTokenJson(),
                        clientId: authService.clientId,
                        clientSecret: authService.clientSecret
                    )
                } catch {
                    OrionLogger.sync.error("Sync failed for \(account.email): \(error)")
                }
            }
            await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
        }
    }

    private func fetchUserEmail(accessToken: String) async throws -> String {
        var request = URLRequest(url: URL(string: "https://www.googleapis.com/gmail/v1/users/me/profile")!)
        request.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse,
              httpResponse.statusCode == 200 else {
            throw AuthError.tokenExchangeFailed
        }

        let json = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        guard let email = json?["emailAddress"] as? String else {
            throw AuthError.unknown
        }

        return email
    }

    private var currentThread: FfiThreadSummary? {
        guard selectedThreadIndex < mailBridge.threads.count else { return nil }
        return mailBridge.threads[selectedThreadIndex]
    }

    private var currentLabelTitle: String {
        switch selectedLabel {
        case "INBOX": return "Inbox"
        case "STARRED": return "Starred"
        case "SENT": return "Sent"
        case "DRAFT": return "Drafts"
        case "ALL": return "All Mail"
        case "SPAM": return "Spam"
        case "TRASH": return "Trash"
        case .none: return "Inbox"
        default: return selectedLabel ?? "Mail"
        }
    }
}

extension Notification.Name {
    static let showKeyboardShortcuts = Notification.Name("showKeyboardShortcuts")
}
