import SwiftUI

/// List of email threads with virtual scrolling
struct ThreadListView: View {
    @EnvironmentObject var mailBridge: MailBridge

    let label: String?
    let accountId: Int64?
    let onSelectThread: (FfiThreadSummary) -> Void

    @State private var selectedThreadId: String? = nil
    @State private var hoveredThreadId: String? = nil

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(mailBridge.threads) { thread in
                    ThreadListItem(
                        thread: thread,
                        isSelected: selectedThreadId == thread.id,
                        isHovered: hoveredThreadId == thread.id,
                        showAccount: accountId == nil && mailBridge.accounts.count > 1
                    )
                    .onTapGesture {
                        selectedThreadId = thread.id
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
        .background(OrionTheme.background)
        .overlay {
            // Only show empty state when not loading and no threads
            if !mailBridge.isLoading && mailBridge.threads.isEmpty && mailBridge.isInitialized {
                emptyState
            }
        }
        // Use task(id:) to coalesce loads - include isInitialized so we reload after init completes
        .task(id: "\(mailBridge.isInitialized)-\(label ?? "nil")-\(accountId.map(String.init) ?? "nil")") {
            guard mailBridge.isInitialized else { return }
            await mailBridge.loadThreads(label: label, accountId: accountId)
        }
    }

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
        HStack(spacing: OrionTheme.spacing2) {
            // Unread indicator
            Circle()
                .fill(thread.isUnread ? OrionTheme.primary : Color.clear)
                .frame(width: OrionTheme.unreadDotSize, height: OrionTheme.unreadDotSize)

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
