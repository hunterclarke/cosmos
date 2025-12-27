import SwiftUI

/// Main content view with sidebar + content layout
struct ContentView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService

    @State private var selectedLabel: String? = "INBOX"
    @State private var selectedAccountId: Int64? = nil
    @State private var selectedThread: FfiThreadSummary? = nil
    @State private var searchQuery: String = ""
    @State private var isSearching: Bool = false
    @State private var showShortcutsHelp: Bool = false

    var body: some View {
        NavigationSplitView {
            SidebarView(
                selectedLabel: $selectedLabel,
                selectedAccountId: $selectedAccountId
            )
            .frame(width: 240)
        } detail: {
            ZStack {
                // Keep ThreadListView alive to avoid reload on navigation back
                ThreadListView(
                    label: selectedLabel,
                    accountId: selectedAccountId,
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
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(OrionTheme.background)
            .toolbar {
                ToolbarItem(placement: .automatic) {
                    SearchBox(
                        query: $searchQuery,
                        isSearching: $isSearching
                    )
                }
            }
        }
        .navigationSplitViewStyle(.balanced)
        .onAppear {
            Task {
                await mailBridge.initialize()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .showKeyboardShortcuts)) { _ in
            showShortcutsHelp.toggle()
        }
    }
}

extension Notification.Name {
    static let showKeyboardShortcuts = Notification.Name("showKeyboardShortcuts")
}
