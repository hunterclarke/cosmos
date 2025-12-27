import SwiftUI

/// Detail view showing a thread with all its messages
/// Adapts to iOS with a floating action bar at the bottom
struct ThreadDetailView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService
    @Environment(\.horizontalSizeClass) var horizontalSizeClass

    let thread: FfiThreadSummary
    let onBack: () -> Void

    // Optional action callbacks for iOS NavigationStack integration
    var onArchive: (() -> Void)?
    var onStar: (() -> Void)?
    var onToggleRead: (() -> Void)?

    @State private var threadDetail: FfiThreadDetail? = nil
    @State private var isLoading: Bool = true
    @State private var isStarred: Bool = false
    @State private var isRead: Bool = true
    @State private var showingError: Bool = false
    @State private var errorMessage: String = ""

    var body: some View {
        VStack(spacing: 0) {
            #if os(macOS)
            // Header with actions (macOS only - iOS uses navigation bar)
            headerView

            Divider()
                .background(OrionTheme.border)
            #endif

            // Messages
            if isLoading {
                Spacer()
                ProgressView()
                Spacer()
            } else if let detail = threadDetail {
                ScrollView {
                    LazyVStack(spacing: OrionTheme.spacing3) {
                        ForEach(detail.messages) { message in
                            MessageCard(message: message)
                        }
                    }
                    .padding(OrionTheme.spacing4)
                }
            } else {
                Spacer()
                Text("Failed to load thread")
                    .foregroundColor(OrionTheme.mutedForeground)
                Spacer()
            }
        }
        .background(OrionTheme.background)
        #if os(iOS)
        .navigationTitle(thread.subject)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                // Compact toolbar for iPad regular size class
                if horizontalSizeClass == .regular {
                    actionButtons
                }
            }
        }
        .safeAreaInset(edge: .bottom) {
            // Floating action bar for iPhone
            if horizontalSizeClass == .compact {
                iOSActionBar
            }
        }
        #endif
        .task {
            await loadThreadDetail()

            // Set star state from message labels (FfiMessage includes labelIds)
            isStarred = threadDetail?.messages.contains { $0.labelIds.contains("STARRED") } ?? false
            isRead = !thread.isUnread

            // Auto-mark as read if currently unread
            if thread.isUnread {
                do {
                    let tokens = try await authService.getValidTokens(for: thread.accountId)
                    try await mailBridge.setRead(
                        threadId: thread.id,
                        isRead: true,
                        tokenJson: tokens.toTokenJson(),
                        clientId: authService.clientId,
                        clientSecret: authService.clientSecret
                    )
                    isRead = true
                } catch {
                    OrionLogger.ui.error("Auto-mark read failed: \(error)")
                }
            }
        }
        .alert("Error", isPresented: $showingError) {
            Button("OK", role: .cancel) { }
        } message: {
            Text(errorMessage)
        }
    }

    // MARK: - iOS Action Bar

    #if os(iOS)
    private var iOSActionBar: some View {
        HStack(spacing: 0) {
            // Archive
            Button {
                if let onArchive = onArchive {
                    onArchive()
                } else {
                    Task { await archiveThread() }
                }
            } label: {
                VStack(spacing: 4) {
                    Image(systemName: "archivebox")
                        .font(.system(size: 20))
                    Text("Archive")
                        .font(.system(size: 10))
                }
                .frame(maxWidth: .infinity)
                .foregroundColor(OrionTheme.foreground)
            }

            // Star
            Button {
                if let onStar = onStar {
                    onStar()
                } else {
                    Task { await toggleStar() }
                }
            } label: {
                VStack(spacing: 4) {
                    Image(systemName: isStarred ? "star.fill" : "star")
                        .font(.system(size: 20))
                        .foregroundColor(isStarred ? .yellow : OrionTheme.foreground)
                    Text(isStarred ? "Unstar" : "Star")
                        .font(.system(size: 10))
                }
                .frame(maxWidth: .infinity)
                .foregroundColor(OrionTheme.foreground)
            }

            // Read/Unread
            Button {
                if let onToggleRead = onToggleRead {
                    onToggleRead()
                } else {
                    Task { await toggleRead() }
                }
            } label: {
                VStack(spacing: 4) {
                    Image(systemName: isRead ? "envelope.badge" : "envelope.open")
                        .font(.system(size: 20))
                    Text(isRead ? "Unread" : "Read")
                        .font(.system(size: 10))
                }
                .frame(maxWidth: .infinity)
                .foregroundColor(OrionTheme.foreground)
            }

            // Trash
            Button {
                Task { await trashThread() }
            } label: {
                VStack(spacing: 4) {
                    Image(systemName: "trash")
                        .font(.system(size: 20))
                    Text("Trash")
                        .font(.system(size: 10))
                }
                .frame(maxWidth: .infinity)
                .foregroundColor(OrionTheme.foreground)
            }
        }
        .padding(.vertical, 8)
        .background(.ultraThinMaterial)
        .overlay(
            Divider()
                .background(OrionTheme.border),
            alignment: .top
        )
    }
    #endif

    // MARK: - Header

    private var headerView: some View {
        HStack(spacing: OrionTheme.spacing3) {
            // Back button
            Button(action: onBack) {
                Image(systemName: "chevron.left")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundColor(OrionTheme.foreground)
            }
            .buttonStyle(.plain)
            .keyboardShortcut(.escape, modifiers: [])

            // Subject
            Text(thread.subject)
                .font(.system(size: OrionTheme.textLg, weight: .semibold))
                .foregroundColor(OrionTheme.foreground)
                .lineLimit(1)

            Spacer()

            // Action buttons
            actionButtons
        }
        .padding(.horizontal, OrionTheme.spacing4)
        .padding(.vertical, OrionTheme.spacing3)
        .background(OrionTheme.secondary)
    }

    private var actionButtons: some View {
        HStack(spacing: OrionTheme.spacing2) {
            ActionButton(icon: "archivebox", tooltip: "Archive (e)") {
                await archiveThread()
            }
            .keyboardShortcut("e", modifiers: [])

            ActionButton(icon: isStarred ? "star.fill" : "star", tooltip: isStarred ? "Unstar (s)" : "Star (s)") {
                await toggleStar()
            }
            .keyboardShortcut("s", modifiers: [])

            ActionButton(
                icon: isRead ? "envelope" : "envelope.open",
                tooltip: isRead ? "Mark as unread" : "Mark as read"
            ) {
                await toggleRead()
            }

            ActionButton(icon: "trash", tooltip: "Delete (#)") {
                await trashThread()
            }
            .keyboardShortcut("#", modifiers: [])
        }
    }

    // MARK: - Actions

    private func loadThreadDetail() async {
        isLoading = true
        threadDetail = await mailBridge.loadThreadDetail(threadId: thread.id)
        isLoading = false
    }

    private func getTokensForThread() async throws -> OAuthTokens {
        return try await authService.getValidTokens(for: thread.accountId)
    }

    private func archiveThread() async {
        do {
            let tokens = try await getTokensForThread()
            try await mailBridge.archiveThread(
                threadId: thread.id,
                tokenJson: tokens.toTokenJson(),
                clientId: authService.clientId,
                clientSecret: authService.clientSecret
            )
            // Go back to list after archiving
            onBack()
        } catch {
            errorMessage = "Archive failed: \(error.localizedDescription)"
            showingError = true
        }
    }

    private func toggleStar() async {
        do {
            let tokens = try await getTokensForThread()
            let newStarred = try await mailBridge.toggleStar(
                threadId: thread.id,
                tokenJson: tokens.toTokenJson(),
                clientId: authService.clientId,
                clientSecret: authService.clientSecret
            )
            isStarred = newStarred
        } catch {
            errorMessage = "Star toggle failed: \(error.localizedDescription)"
            showingError = true
        }
    }

    private func toggleRead() async {
        do {
            let tokens = try await getTokensForThread()
            let newReadState = !isRead
            try await mailBridge.setRead(
                threadId: thread.id,
                isRead: newReadState,
                tokenJson: tokens.toTokenJson(),
                clientId: authService.clientId,
                clientSecret: authService.clientSecret
            )
            isRead = newReadState
        } catch {
            errorMessage = "Read toggle failed: \(error.localizedDescription)"
            showingError = true
        }
    }

    private func trashThread() async {
        do {
            let tokens = try await getTokensForThread()
            try await mailBridge.trashThread(
                threadId: thread.id,
                tokenJson: tokens.toTokenJson(),
                clientId: authService.clientId,
                clientSecret: authService.clientSecret
            )
            // Go back to list after trashing
            onBack()
        } catch {
            errorMessage = "Trash failed: \(error.localizedDescription)"
            showingError = true
        }
    }
}

// MARK: - Action Button

struct ActionButton: View {
    let icon: String
    let tooltip: String
    let action: () async -> Void

    @State private var isHovered: Bool = false

    var body: some View {
        Button {
            Task { await action() }
        } label: {
            Image(systemName: icon)
                .font(.system(size: 14))
                .foregroundColor(isHovered ? OrionTheme.foreground : OrionTheme.mutedForeground)
                .frame(width: 28, height: 28)
                .background(isHovered ? OrionTheme.listHover : Color.clear)
                .cornerRadius(4)
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovered = hovering
        }
        .help(tooltip)
    }
}

// MARK: - Message Card

struct MessageCard: View {
    let message: FfiMessage

    @State private var isExpanded: Bool = true

    private var formattedDate: String {
        let date = Date(timeIntervalSince1970: TimeInterval(message.receivedAt))
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        return formatter.string(from: date)
    }

    private var senderDisplay: String {
        if let name = message.from.name, !name.isEmpty {
            return "\(name) <\(message.from.email)>"
        }
        return message.from.email
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(alignment: .top, spacing: OrionTheme.spacing2) {
                // Avatar
                Circle()
                    .fill(OrionTheme.primary)
                    .frame(width: 32, height: 32)
                    .overlay(
                        Text(String((message.from.name ?? message.from.email).prefix(1)).uppercased())
                            .font(.system(size: 14, weight: .medium))
                            .foregroundColor(.white)
                    )

                // Sender info
                VStack(alignment: .leading, spacing: 2) {
                    Text(message.from.name ?? message.from.email)
                        .font(.system(size: OrionTheme.textSm, weight: .semibold))
                        .foregroundColor(OrionTheme.foreground)

                    if !message.to.isEmpty {
                        Text("to \(message.to.map { $0.name ?? $0.email }.joined(separator: ", "))")
                            .font(.system(size: OrionTheme.textXs))
                            .foregroundColor(OrionTheme.mutedForeground)
                            .lineLimit(1)
                    }
                }

                Spacer()

                // Date
                Text(formattedDate)
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)

                // Expand/collapse
                Button {
                    withAnimation(.easeInOut(duration: 0.2)) {
                        isExpanded.toggle()
                    }
                } label: {
                    Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                        .font(.system(size: 10))
                        .foregroundColor(OrionTheme.mutedForeground)
                }
                .buttonStyle(.plain)
            }
            .padding(OrionTheme.spacing3)

            // Body
            if isExpanded {
                Divider()
                    .background(OrionTheme.border)

                messageBody
                    .padding(OrionTheme.spacing3)
            }
        }
        .background(OrionTheme.secondary)
        .cornerRadius(8)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .stroke(OrionTheme.border, lineWidth: 1)
        )
    }

    @ViewBuilder
    private var messageBody: some View {
        if let html = message.bodyHtml, !html.isEmpty {
            // Render HTML content with WKWebView
            // iOS: Allow content to determine height (no maxHeight)
            // macOS: Use constrained height
            #if os(iOS)
            MessageWebView(html: html)
                .frame(minHeight: 200)
            #else
            MessageWebView(html: html)
                .frame(minHeight: 100, maxHeight: 600)
            #endif
        } else if let text = message.bodyText, !text.isEmpty {
            Text(text)
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.foreground)
                .textSelection(.enabled)
        } else {
            Text(message.bodyPreview)
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.foreground)
                .textSelection(.enabled)
        }
    }
}
