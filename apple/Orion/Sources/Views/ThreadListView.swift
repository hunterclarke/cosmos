import SwiftUI

/// List of email threads with virtual scrolling
/// Supports swipe actions on iOS and pull-to-refresh
struct ThreadListView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService

    let label: String?
    let accountId: Int64?
    @Binding var selectedIndex: Int
    let onSelectThread: (FfiThreadSummary) -> Void

    // Optional swipe action callbacks (used on iOS)
    var onArchive: ((FfiThreadSummary) -> Void)?
    var onStar: ((FfiThreadSummary) -> Void)?
    var onToggleRead: ((FfiThreadSummary) -> Void)?

    @State private var hoveredThreadId: String? = nil

    var body: some View {
        ScrollViewReader { proxy in
            #if os(iOS)
            List {
                ForEach(Array(mailBridge.threads.enumerated()), id: \.element.id) { index, thread in
                    ThreadListItem(
                        thread: thread,
                        isSelected: selectedIndex == index,
                        isHovered: false,
                        showAccount: accountId == nil && mailBridge.accounts.count > 1,
                        compact: true
                    )
                    .id(index)
                    .listRowInsets(EdgeInsets())
                    .listRowSeparator(.hidden)
                    .contentShape(Rectangle())
                    .onTapGesture {
                        selectedIndex = index
                        onSelectThread(thread)
                    }
                    .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                        if let onArchive = onArchive {
                            Button {
                                onArchive(thread)
                            } label: {
                                Label("Archive", systemImage: "archivebox")
                            }
                            .tint(.orange)
                        }
                    }
                    .swipeActions(edge: .leading, allowsFullSwipe: true) {
                        if let onStar = onStar {
                            Button {
                                onStar(thread)
                            } label: {
                                Label("Star", systemImage: "star.fill")
                            }
                            .tint(.yellow)
                        }

                        if let onToggleRead = onToggleRead {
                            Button {
                                onToggleRead(thread)
                            } label: {
                                Label(thread.isUnread ? "Read" : "Unread", systemImage: thread.isUnread ? "envelope.open" : "envelope.badge")
                            }
                            .tint(.blue)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(OrionTheme.background)
            .refreshable {
                await syncAllAccounts()
            }
            .onChange(of: selectedIndex) { _, newIndex in
                withAnimation {
                    proxy.scrollTo(newIndex, anchor: .center)
                }
            }
            #else
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(Array(mailBridge.threads.enumerated()), id: \.element.id) { index, thread in
                        ThreadListItem(
                            thread: thread,
                            isSelected: selectedIndex == index,
                            isHovered: hoveredThreadId == thread.id,
                            showAccount: accountId == nil && mailBridge.accounts.count > 1
                        )
                        .id(index)
                        .onTapGesture {
                            selectedIndex = index
                            onSelectThread(thread)
                        }
                        .onHover { isHovered in
                            hoveredThreadId = isHovered ? thread.id : nil
                        }

                        Divider()
                            .background(OrionTheme.border)
                    }
                }
            }
            .onChange(of: selectedIndex) { _, newIndex in
                withAnimation {
                    proxy.scrollTo(newIndex, anchor: .center)
                }
            }
            #endif
        }
        .background(OrionTheme.background)
        .overlay {
            // Only show empty state when not loading, not syncing, and no threads
            if !mailBridge.isLoading && !mailBridge.isSyncing && mailBridge.threads.isEmpty && mailBridge.isInitialized {
                emptyState
            }
        }
        // Use task(id:) to coalesce loads - include isInitialized so we reload after init completes
        .task(id: "\(mailBridge.isInitialized)-\(label ?? "nil")-\(accountId.map(String.init) ?? "nil")") {
            guard mailBridge.isInitialized else { return }
            await mailBridge.loadThreads(label: label, accountId: accountId)
        }
    }

    // MARK: - Pull-to-Refresh Sync

    #if os(iOS)
    private func syncAllAccounts() async {
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
                OrionLogger.ui.error("Sync failed for \(account.email): \(error)")
            }
        }
        await mailBridge.loadThreads(label: label, accountId: accountId)
    }
    #endif

    private var emptyState: some View {
        VStack(spacing: OrionTheme.spacing3) {
            Image(systemName: "tray")
                .font(.system(size: 48))
                .foregroundColor(OrionTheme.mutedForeground)

            Text("No emails")
                .font(.system(size: OrionTheme.textLg))
                .foregroundColor(OrionTheme.foreground)

            Text("Your inbox is empty")
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.mutedForeground)
        }
    }

}

// MARK: - Thread List Item

struct ThreadListItem: View {
    let thread: FfiThreadSummary
    let isSelected: Bool
    let isHovered: Bool
    let showAccount: Bool
    var compact: Bool = false  // Compact layout for iOS

    private var formattedDate: String {
        let date = Date(timeIntervalSince1970: TimeInterval(thread.lastMessageAt))
        let formatter = DateFormatter()

        let calendar = Calendar.current
        if calendar.isDateInToday(date) {
            formatter.dateFormat = "h:mm a"
        } else if calendar.isDate(date, equalTo: Date(), toGranularity: .year) {
            formatter.dateFormat = "MMM d"
        } else {
            formatter.dateFormat = "MM/dd/yy"
        }

        return formatter.string(from: date)
    }

    private var senderDisplay: String {
        thread.senderName ?? thread.senderEmail.components(separatedBy: "@").first ?? thread.senderEmail
    }

    var body: some View {
        if compact {
            compactLayout
        } else {
            wideLayout
        }
    }

    // MARK: - Compact Layout (iOS)

    private var compactLayout: some View {
        VStack(alignment: .leading, spacing: 4) {
            // Top row: Sender + Date
            HStack {
                // Unread indicator + Sender
                HStack(spacing: 8) {
                    // Only show dot when unread
                    if thread.isUnread {
                        Circle()
                            .fill(OrionTheme.primary)
                            .frame(width: 8, height: 8)
                    }

                    Text(senderDisplay)
                        .font(.system(size: 15, weight: thread.isUnread ? .semibold : .regular))
                        .foregroundColor(OrionTheme.foreground)
                        .lineLimit(1)

                    // Message count badge
                    if thread.messageCount > 1 {
                        Text("\(thread.messageCount)")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundColor(OrionTheme.mutedForeground)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(OrionTheme.secondary)
                            .clipShape(Capsule())
                    }
                }

                Spacer()

                // Date (star status not available in FfiThreadSummary)
                HStack(spacing: 8) {
                    Text(formattedDate)
                        .font(.system(size: 12))
                        .foregroundColor(OrionTheme.mutedForeground)
                }
            }

            // Subject
            Text(thread.subject)
                .font(.system(size: 14, weight: thread.isUnread ? .medium : .regular))
                .foregroundColor(OrionTheme.foreground)
                .lineLimit(1)

            // Snippet
            Text(thread.snippet)
                .font(.system(size: 13))
                .foregroundColor(OrionTheme.mutedForeground)
                .lineLimit(2)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(backgroundColor)
    }

    // MARK: - Wide Layout (macOS/iPad)

    private var wideLayout: some View {
        HStack(spacing: OrionTheme.spacing2) {
            // Unread indicator - only show when unread
            if thread.isUnread {
                Circle()
                    .fill(OrionTheme.primary)
                    .frame(width: OrionTheme.unreadDotSize, height: OrionTheme.unreadDotSize)
            }

            // Sender
            Text(senderDisplay)
                .font(.system(size: OrionTheme.textSm, weight: thread.isUnread ? .semibold : .regular))
                .foregroundColor(OrionTheme.foreground)
                .lineLimit(1)
                .frame(width: OrionTheme.senderColumnWidth, alignment: .leading)

            // Subject and snippet
            HStack(spacing: OrionTheme.spacing1) {
                Text(thread.subject)
                    .font(.system(size: OrionTheme.textSm, weight: thread.isUnread ? .medium : .regular))
                    .foregroundColor(OrionTheme.foreground)
                    .lineLimit(1)

                Text("—")
                    .foregroundColor(OrionTheme.mutedForeground)

                Text(thread.snippet)
                    .font(.system(size: OrionTheme.textSm))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .lineLimit(1)
            }

            Spacer()

            // Message count (if > 1)
            if thread.messageCount > 1 {
                Text("(\(thread.messageCount))")
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)
            }

            // Date
            Text(formattedDate)
                .font(.system(size: OrionTheme.textXs))
                .foregroundColor(OrionTheme.mutedForeground)
                .frame(width: 60, alignment: .trailing)
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .frame(height: OrionTheme.threadItemHeight)
        .background(backgroundColor)
        .overlay(
            Rectangle()
                .fill(isSelected ? OrionTheme.listActiveBorder : Color.clear)
                .frame(width: 3),
            alignment: .leading
        )
    }

    private var backgroundColor: Color {
        if isSelected {
            return OrionTheme.listActive
        } else if isHovered {
            return OrionTheme.listHover
        } else {
            return OrionTheme.list
        }
    }
}
