import SwiftUI

/// Sidebar with account list and label navigation
struct SidebarView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService

    @Binding var selectedLabel: String?
    @Binding var selectedAccountId: Int64?

    @State private var isExpanded: Bool = true
    @State private var showingError: Bool = false
    @State private var errorMessage: String = ""

    // Standard Gmail labels (order matches GPUI app)
    private let labels: [(id: String, name: String, icon: String)] = [
        ("INBOX", "Inbox", "tray"),
        ("STARRED", "Starred", "star"),
        ("SENT", "Sent", "paperplane"),
        ("DRAFT", "Drafts", "doc"),
        ("ALL", "All Mail", "folder"),
        ("SPAM", "Spam", "exclamationmark.shield"),
        ("TRASH", "Trash", "trash")
    ]

    var body: some View {
        VStack(spacing: 0) {
            // Accounts section
            accountsSection

            Divider()
                .background(OrionTheme.border)

            // Labels section
            labelsSection

            Spacer()

            // Sync footer
            syncFooter
        }
        .frame(maxHeight: .infinity)
        .background(OrionTheme.secondary)
    }

    // MARK: - Accounts Section

    private var accountsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Section header
            HStack {
                Text("Accounts")
                    .font(.system(size: OrionTheme.textXs, weight: .medium))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .textCase(.uppercase)

                Spacer()

                Button(action: { isExpanded.toggle() }) {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 10))
                        .foregroundColor(OrionTheme.mutedForeground)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, OrionTheme.spacing3)
            .padding(.vertical, OrionTheme.spacing2)

            if isExpanded {
                // "All Accounts" option
                AccountRowView(
                    name: "All Accounts",
                    email: nil,
                    color: OrionTheme.primary,
                    isSelected: selectedAccountId == nil,
                    unreadCount: totalUnreadCount
                )
                .onTapGesture {
                    selectedAccountId = nil
                }

                // Individual accounts
                ForEach(mailBridge.accounts) { account in
                    AccountRowView(
                        name: account.displayName ?? account.email,
                        email: account.email,
                        color: Color(hex: account.avatarColor),
                        isSelected: selectedAccountId == account.id,
                        unreadCount: 0 // TODO: Per-account unread count
                    )
                    .onTapGesture {
                        selectedAccountId = account.id
                    }
                }

                // Add account button
                Button(action: addAccount) {
                    HStack(spacing: OrionTheme.spacing2) {
                        Image(systemName: "plus.circle")
                            .font(.system(size: 14))
                        Text("Add Account")
                            .font(.system(size: OrionTheme.textSm))
                    }
                    .foregroundColor(OrionTheme.mutedForeground)
                    .padding(.horizontal, OrionTheme.spacing3)
                    .padding(.vertical, OrionTheme.spacing2)
                }
                .buttonStyle(.plain)
                .disabled(!authService.isConfigured || !mailBridge.isInitialized)
            }
        }
    }

    // MARK: - Labels Section

    private var labelsSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(labels, id: \.id) { label in
                LabelRowView(
                    name: label.name,
                    icon: label.icon,
                    isSelected: selectedLabel == label.id,
                    unreadCount: labelUnreadCount(for: label.id)
                )
                .onTapGesture {
                    selectedLabel = label.id
                }
            }
        }
        .padding(.top, OrionTheme.spacing2)
        .task {
            // Load label unread counts on appear
            await mailBridge.loadLabelUnreadCounts(accountId: selectedAccountId)
        }
    }

    // MARK: - Sync Footer

    private var syncFooter: some View {
        VStack(spacing: OrionTheme.spacing1) {
            if !authService.isConfigured {
                Text("OAuth not configured")
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .multilineTextAlignment(.center)
            } else if mailBridge.isSyncing {
                HStack(spacing: OrionTheme.spacing2) {
                    ProgressView()
                        .scaleEffect(0.7)

                    if let progress = mailBridge.syncProgress {
                        Text(progress.phase)
                            .font(.system(size: OrionTheme.textXs))
                            .foregroundColor(OrionTheme.mutedForeground)
                    }
                }
            } else if authService.isAuthenticating {
                HStack(spacing: OrionTheme.spacing2) {
                    ProgressView()
                        .scaleEffect(0.7)
                    Text("Authenticating...")
                        .font(.system(size: OrionTheme.textXs))
                        .foregroundColor(OrionTheme.mutedForeground)
                }
            }

            Button(action: syncAccounts) {
                HStack(spacing: OrionTheme.spacing2) {
                    Image(systemName: "arrow.clockwise")
                        .font(.system(size: 12))
                    Text("Sync")
                        .font(.system(size: OrionTheme.textSm))
                }
                .foregroundColor(OrionTheme.secondaryForeground)
                .padding(.horizontal, OrionTheme.spacing3)
                .padding(.vertical, OrionTheme.spacing2)
            }
            .buttonStyle(.plain)
            .disabled(mailBridge.isSyncing || !authService.isConfigured || mailBridge.accounts.isEmpty)
        }
        .padding(.vertical, OrionTheme.spacing2)
        .alert("Error", isPresented: $showingError) {
            Button("OK", role: .cancel) { }
        } message: {
            Text(errorMessage)
        }
    }

    // MARK: - Computed Properties

    private var totalUnreadCount: Int {
        // Use inbox unread count from cache, or fall back to current unread count
        Int(mailBridge.labelUnreadCounts["INBOX"] ?? mailBridge.unreadCount)
    }

    private func labelUnreadCount(for labelId: String) -> Int? {
        // Return cached unread count for this label
        if let count = mailBridge.labelUnreadCounts[labelId], count > 0 {
            return Int(count)
        }
        return nil
    }

    // MARK: - Actions

    private func addAccount() {
        guard authService.isConfigured else {
            errorMessage = "OAuth not configured. Please add google-credentials.json to ~/Library/Application Support/cosmos/"
            showingError = true
            return
        }

        guard mailBridge.isInitialized else {
            errorMessage = "Mail service not initialized. Please wait..."
            showingError = true
            return
        }

        Task {
            do {
                // Start OAuth flow
                let tokens = try await authService.authenticate()

                // Get user's email from Google
                let email = try await fetchUserEmail(accessToken: tokens.accessToken)

                // Register account with MailBridge
                if let account = await mailBridge.addAccount(email: email) {
                    // Save tokens for this account
                    try authService.saveTokens(tokens, for: account.id)
                    OrionLogger.ui.info("Added account: \(email)")

                    // Trigger initial sync
                    await syncAccount(account, tokens: tokens)

                    // Reload threads after sync
                    await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
                } else {
                    errorMessage = "Failed to register account"
                    showingError = true
                }
            } catch {
                errorMessage = error.localizedDescription
                showingError = true
            }
        }
    }

    private func syncAccounts() {
        Task {
            for account in mailBridge.accounts {
                do {
                    let tokens = try await authService.getValidTokens(for: account.id)
                    await syncAccount(account, tokens: tokens)
                } catch {
                    OrionLogger.ui.error("Failed to get tokens for \(account.email): \(error)")
                    // Continue with other accounts
                }
            }

            // Reload threads after sync
            await mailBridge.loadThreads(label: selectedLabel, accountId: selectedAccountId)
        }
    }

    private func syncAccount(_ account: FfiAccount, tokens: OAuthTokens) async {
        do {
            let stats = try await mailBridge.syncAccount(
                accountId: account.id,
                tokenJson: tokens.toTokenJson(),
                clientId: authService.clientId,
                clientSecret: authService.clientSecret
            )
            OrionLogger.sync.info("Synced \(account.email): \(stats.messagesFetched) messages")
        } catch {
            OrionLogger.sync.error("Sync failed for \(account.email): \(error)")
            errorMessage = "Sync failed: \(error.localizedDescription)"
            showingError = true
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
}

// MARK: - Account Row

struct AccountRowView: View {
    let name: String
    let email: String?
    let color: Color
    let isSelected: Bool
    let unreadCount: Int

    var body: some View {
        HStack(spacing: OrionTheme.spacing2) {
            // Avatar
            Circle()
                .fill(color)
                .frame(width: OrionTheme.avatarSize, height: OrionTheme.avatarSize)
                .overlay(
                    Text(String(name.prefix(1)).uppercased())
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(.white)
                )

            // Name and email
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .font(.system(size: OrionTheme.textSm))
                    .foregroundColor(OrionTheme.foreground)
                    .lineLimit(1)

                if let email = email {
                    Text(email)
                        .font(.system(size: OrionTheme.textXs))
                        .foregroundColor(OrionTheme.mutedForeground)
                        .lineLimit(1)
                }
            }

            Spacer()

            // Unread badge
            if unreadCount > 0 {
                Text("\(unreadCount)")
                    .font(.system(size: OrionTheme.textXs, weight: .medium))
                    .foregroundColor(OrionTheme.primaryForeground)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(OrionTheme.primary)
                    .clipShape(Capsule())
            }
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .padding(.vertical, OrionTheme.spacing2)
        .contentShape(Rectangle()) // Make entire row tappable
        .background(isSelected ? OrionTheme.listActive : Color.clear)
        .overlay(
            Rectangle()
                .fill(isSelected ? OrionTheme.listActiveBorder : Color.clear)
                .frame(width: 3),
            alignment: .leading
        )
    }
}

// MARK: - Label Row

struct LabelRowView: View {
    let name: String
    let icon: String
    let isSelected: Bool
    let unreadCount: Int?

    var body: some View {
        HStack(spacing: OrionTheme.spacing2) {
            Image(systemName: icon)
                .font(.system(size: 14))
                .foregroundColor(isSelected ? OrionTheme.foreground : OrionTheme.mutedForeground)
                .frame(width: 20)

            Text(name)
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(isSelected ? OrionTheme.foreground : OrionTheme.secondaryForeground)

            Spacer()

            if let count = unreadCount, count > 0 {
                Text("\(count)")
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)
            }
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .padding(.vertical, OrionTheme.spacing2)
        .contentShape(Rectangle()) // Make entire row tappable
        .background(isSelected ? OrionTheme.listActive : Color.clear)
        .overlay(
            Rectangle()
                .fill(isSelected ? OrionTheme.listActiveBorder : Color.clear)
                .frame(width: 3),
            alignment: .leading
        )
    }
}
