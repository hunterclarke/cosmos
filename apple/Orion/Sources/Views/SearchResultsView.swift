import SwiftUI

/// Search results view with highlighting
struct SearchResultsView: View {
    @EnvironmentObject var mailBridge: MailBridge

    let query: String
    let onSelectThread: (FfiSearchResult) -> Void

    @State private var selectedResultId: String? = nil
    @State private var hoveredResultId: String? = nil

    private var trimmedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(spacing: 0) {
            // Show empty state if no query
            if trimmedQuery.isEmpty {
                emptyQueryView
            } else {
                searchResultsContent
            }
        }
        .task {
            guard !trimmedQuery.isEmpty else { return }
            await mailBridge.search(query: trimmedQuery)
        }
        .onChange(of: query) { _, newQuery in
            let trimmed = newQuery.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty else {
                // Clear results when query is empty
                mailBridge.searchResults = []
                return
            }
            Task {
                await mailBridge.search(query: trimmed)
            }
        }
    }

    private var searchResultsContent: some View {
        VStack(spacing: 0) {
            // Search header
            HStack {
                Text("Search results for")
                    .font(.system(size: OrionTheme.textSm))
                    .foregroundColor(OrionTheme.mutedForeground)

                Text("\"\(trimmedQuery)\"")
                    .font(.system(size: OrionTheme.textSm, weight: .medium))
                    .foregroundColor(OrionTheme.foreground)

                Spacer()

                Text("\(mailBridge.searchResults.count) results")
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)
            }
            .padding(.horizontal, OrionTheme.spacing4)
            .padding(.vertical, OrionTheme.spacing2)
            .background(OrionTheme.secondary)

            Divider()
                .background(OrionTheme.border)

            // Results list
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(mailBridge.searchResults) { result in
                        SearchResultItem(
                            result: result,
                            isSelected: selectedResultId == result.id,
                            isHovered: hoveredResultId == result.id
                        )
                        .onTapGesture {
                            selectedResultId = result.id
                            onSelectThread(result)
                        }
                        .onHover { isHovered in
                            hoveredResultId = isHovered ? result.id : nil
                        }

                        Divider()
                            .background(OrionTheme.border)
                    }
                }
            }
            .background(OrionTheme.background)
            .overlay {
                if mailBridge.isLoading {
                    ProgressView()
                        .scaleEffect(1.2)
                } else if mailBridge.searchResults.isEmpty {
                    noResultsView
                }
            }
        }
    }

    private var emptyQueryView: some View {
        VStack(spacing: OrionTheme.spacing3) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 48))
                .foregroundColor(OrionTheme.mutedForeground)

            Text("Search your emails")
                .font(.system(size: OrionTheme.textLg))
                .foregroundColor(OrionTheme.foreground)

            Text("Enter keywords, sender names, or use operators like from:, to:, is:unread")
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.mutedForeground)
                .multilineTextAlignment(.center)
                .padding(.horizontal, OrionTheme.spacing4)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(OrionTheme.background)
    }

    private var noResultsView: some View {
        VStack(spacing: OrionTheme.spacing3) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 48))
                .foregroundColor(OrionTheme.mutedForeground)

            Text("No results found")
                .font(.system(size: OrionTheme.textLg))
                .foregroundColor(OrionTheme.foreground)

            Text("Try different keywords or search operators")
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.mutedForeground)
        }
    }
}

// MARK: - Search Result Item

struct SearchResultItem: View {
    let result: FfiSearchResult
    let isSelected: Bool
    let isHovered: Bool

    private var formattedDate: String {
        let date = Date(timeIntervalSince1970: TimeInterval(result.lastMessageAt))
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
        result.senderName ?? result.senderEmail.components(separatedBy: "@").first ?? result.senderEmail
    }

    var body: some View {
        VStack(alignment: .leading, spacing: OrionTheme.spacing1) {
            // Top row: sender, date
            HStack {
                // Unread indicator - only show when unread
                if result.isUnread {
                    Circle()
                        .fill(OrionTheme.primary)
                        .frame(width: OrionTheme.unreadDotSize, height: OrionTheme.unreadDotSize)
                }

                Text(senderDisplay)
                    .font(.system(size: OrionTheme.textSm, weight: result.isUnread ? .semibold : .regular))
                    .foregroundColor(OrionTheme.foreground)
                    .lineLimit(1)

                Spacer()

                if result.messageCount > 1 {
                    Text("(\(result.messageCount))")
                        .font(.system(size: OrionTheme.textXs))
                        .foregroundColor(OrionTheme.mutedForeground)
                }

                Text(formattedDate)
                    .font(.system(size: OrionTheme.textXs))
                    .foregroundColor(OrionTheme.mutedForeground)
            }

            // Subject with highlights
            highlightedSubject

            // Snippet with highlights
            highlightedSnippet
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .padding(.vertical, OrionTheme.spacing2)
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

    @ViewBuilder
    private var highlightedSubject: some View {
        if let highlight = result.highlights.first(where: { $0.field == "subject" }) {
            HighlightedText(text: highlight.text, spans: highlight.highlights)
                .font(.system(size: OrionTheme.textSm, weight: result.isUnread ? .medium : .regular))
        } else {
            Text(result.subject)
                .font(.system(size: OrionTheme.textSm, weight: result.isUnread ? .medium : .regular))
                .foregroundColor(OrionTheme.foreground)
                .lineLimit(1)
        }
    }

    @ViewBuilder
    private var highlightedSnippet: some View {
        if let highlight = result.highlights.first(where: { $0.field == "body" || $0.field == "snippet" }) {
            HighlightedText(text: highlight.text, spans: highlight.highlights)
                .font(.system(size: OrionTheme.textSm))
        } else {
            Text(result.snippet)
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.mutedForeground)
                .lineLimit(2)
        }
    }
}

// MARK: - Highlighted Text

struct HighlightedText: View {
    let text: String
    let spans: [FfiHighlightSpan]

    var body: some View {
        // Build attributed text with highlights
        let attributedText = buildAttributedString()

        Text(attributedText)
            .lineLimit(2)
    }

    private func buildAttributedString() -> AttributedString {
        var result = AttributedString(text)

        // Sort spans by start position
        let sortedSpans = spans.sorted { $0.start < $1.start }

        // Apply highlights
        for span in sortedSpans {
            let start = Int(span.start)
            let end = min(Int(span.end), text.count)

            guard start < end, start < text.count else { continue }

            // Get the range in the AttributedString
            let startIdx = result.index(result.startIndex, offsetByCharacters: start)
            let endIdx = result.index(result.startIndex, offsetByCharacters: end)

            if startIdx < endIdx {
                result[startIdx..<endIdx].backgroundColor = Color(hue: 50/360, saturation: 0.9, brightness: 0.5).opacity(0.4)
                result[startIdx..<endIdx].foregroundColor = OrionTheme.foreground
            }
        }

        return result
    }
}
