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

    // iPhone tab selection
    @State private var selectedTab: Tab = .inbox

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

    enum Tab: Hashable {
        case inbox
        case search
        case accounts
    }

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
            #if os(iOS)
            if horizontalSizeClass == .compact {
                selectedTab = .search
            }
            #endif
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
        TabView(selection: $selectedTab) {
            // Inbox Tab
            NavigationStack {
                ThreadListView(
                    label: selectedLabel,
                    accountId: selectedAccountId,
                    selectedIndex: $selectedThreadIndex,
                    onSelectThread: { thread in
                        selectedThread = thread
                    },
                    onArchive: archiveThread,
                    onStar: starThread,
                    onToggleRead: toggleReadThread
                )
                .navigationTitle(currentLabelTitle)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        HStack(spacing: 8) {
                            labelPicker
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
            .tabItem {
                Label("Inbox", systemImage: "tray")
            }
            .tag(Tab.inbox)

            // Search Tab
            NavigationStack {
                SearchResultsView(
                    query: searchQuery,
                    onSelectThread: { _ in
                        // TODO: Navigate to thread
                    }
                )
                .navigationTitle("Search")
                .searchable(text: $searchQuery, prompt: "Search emails")
            }
            .tabItem {
                Label("Search", systemImage: "magnifyingglass")
            }
            .tag(Tab.search)

            // Accounts Tab
            NavigationStack {
                iPhoneAccountsView
                    .navigationTitle("Accounts")
            }
            .tabItem {
                Label("Accounts", systemImage: "person.2")
            }
            .tag(Tab.accounts)
        }
    }

    private var labelPicker: some View {
        Menu {
            ForEach(Self.labels, id: \.id) { label in
                Button {
                    selectedLabel = label.id
                } label: {
                    if selectedLabel == label.id {
                        Label(label.name, systemImage: "checkmark")
                    } else {
                        Text(label.name)
                    }
                }
            }
        } label: {
            HStack(spacing: 4) {
                Text(currentLabelTitle)
                    .font(.headline)
                Image(systemName: "chevron.down")
                    .font(.caption)
            }
            .foregroundColor(OrionTheme.foreground)
        }
    }

    private var iPhoneAccountsView: some View {
        List {
            Section("Accounts") {
                // All Accounts option
                Button {
                    selectedAccountId = nil
                    selectedTab = .inbox
                } label: {
                    HStack {
                        Circle()
                            .fill(OrionTheme.primary)
                            .frame(width: 32, height: 32)
                            .overlay(
                                Image(systemName: "tray.2")
                                    .font(.system(size: 14))
                                    .foregroundColor(.white)
                            )
                        Text("All Accounts")
                            .foregroundColor(OrionTheme.foreground)
                        Spacer()
                        if selectedAccountId == nil {
                            Image(systemName: "checkmark")
                                .foregroundColor(OrionTheme.primary)
                        }
                    }
                }

                // Individual accounts
                ForEach(mailBridge.accounts) { account in
                    Button {
                        selectedAccountId = account.id
                        selectedTab = .inbox
                    } label: {
                        HStack {
                            Circle()
                                .fill(Color(hex: account.avatarColor))
                                .frame(width: 32, height: 32)
                                .overlay(
                                    Text(String(account.email.prefix(1)).uppercased())
                                        .font(.system(size: 14, weight: .medium))
                                        .foregroundColor(.white)
                                )
                            VStack(alignment: .leading) {
                                Text(account.displayName ?? account.email)
                                    .foregroundColor(OrionTheme.foreground)
                                Text(account.email)
                                    .font(.caption)
                                    .foregroundColor(OrionTheme.mutedForeground)
                            }
                            Spacer()
                            if selectedAccountId == account.id {
                                Image(systemName: "checkmark")
                                    .foregroundColor(OrionTheme.primary)
                            }
                        }
                    }
                }
            }

            Section {
                Button {
                    addAccount()
                } label: {
                    HStack {
                        Image(systemName: "plus.circle")
                        Text("Add Account")
                    }
                }
                .disabled(!authService.isConfigured || !mailBridge.isInitialized)

                Button {
                    syncAllAccounts()
                } label: {
                    HStack {
                        Image(systemName: "arrow.clockwise")
                        Text(mailBridge.isSyncing ? "Syncing..." : "Sync All")
                    }
                }
                .disabled(mailBridge.isSyncing || !authService.isConfigured || mailBridge.accounts.isEmpty || !mailBridge.isInitialized)
            }

            // Settings section
            Section {
                NavigationLink {
                    SettingsView()
                } label: {
                    Label("Settings", systemImage: "gear")
                }

                NavigationLink {
                    KeyboardShortcutsListView()
                } label: {
                    Label("Keyboard Shortcuts", systemImage: "keyboard")
                }
            }
        }
        .listStyle(.insetGrouped)
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
                            onSelectThread: { thread in
                                // Convert search result to thread summary for display
                                selectedThread = nil // Will need conversion
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

                    // Switch to inbox tab to show sync progress (iOS)
                    #if os(iOS)
                    if horizontalSizeClass == .compact {
                        selectedTab = .inbox
                    }
                    #endif

                    // Trigger initial sync
                    let tokenJson = tokens.toTokenJson()
                    OrionLogger.auth.debug("Token JSON: \(tokenJson)")
                    OrionLogger.auth.debug("Token expires at: \(tokens.expiresAt), now: \(Date())")
                    let _ = try await mailBridge.syncAccount(
                        accountId: account.id,
                        tokenJson: tokenJson,
                        clientId: authService.clientId,
                        clientSecret: authService.clientSecret
                    )
                    await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
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
