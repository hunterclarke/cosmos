import SwiftUI

/// Main entry point for the Orion mail application
@main
struct OrionApp: App {
    /// Shared mail bridge that wraps the Rust MailService
    @StateObject private var mailBridge = MailBridge()

    /// Shared authentication service
    @StateObject private var authService = AuthService()

    init() {
        // Initialize Rust logging before any Rust code runs
        #if DEBUG
        initializeRustLogging(debug: true)
        #else
        initializeRustLogging(debug: false)
        #endif

        OrionLogger.app.info("Orion app initializing")
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(mailBridge)
                .environmentObject(authService)
                .preferredColorScheme(.dark)
        }
        #if os(macOS)
        .windowStyle(.hiddenTitleBar)
        .defaultSize(width: 1200, height: 800)
        .commands {
            OrionCommands()
        }
        #endif

        #if os(macOS)
        Settings {
            SettingsView()
                .environmentObject(mailBridge)
                .environmentObject(authService)
        }
        #endif
    }
}

#if os(macOS)
/// Custom menu commands for Orion
struct OrionCommands: Commands {
    var body: some Commands {
        // Replace default Find menu
        CommandGroup(replacing: .textEditing) {
            Button("Search") {
                NotificationCenter.default.post(name: .focusSearch, object: nil)
            }
            .keyboardShortcut("k", modifiers: .command)
        }

        // Help menu
        CommandGroup(replacing: .help) {
            Button("Keyboard Shortcuts") {
                NotificationCenter.default.post(name: .showKeyboardShortcuts, object: nil)
            }
            .keyboardShortcut("/", modifiers: .command)
        }
    }
}
#endif

/// Settings view with account management and app info
struct SettingsView: View {
    @EnvironmentObject var mailBridge: MailBridge
    @EnvironmentObject var authService: AuthService

    var body: some View {
        #if os(iOS)
        NavigationStack {
            settingsContent
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
        }
        #else
        settingsContent
            .frame(minWidth: 400, minHeight: 300)
        #endif
    }

    private var settingsContent: some View {
        List {
            // Accounts Section
            Section("Accounts") {
                ForEach(mailBridge.accounts) { account in
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
                    }
                }

                if mailBridge.accounts.isEmpty {
                    Text("No accounts configured")
                        .foregroundColor(OrionTheme.mutedForeground)
                }
            }

            // About Section
            Section("About") {
                HStack {
                    Text("Version")
                    Spacer()
                    Text(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0")
                        .foregroundColor(OrionTheme.mutedForeground)
                }

                HStack {
                    Text("Build")
                    Spacer()
                    Text(Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "1")
                        .foregroundColor(OrionTheme.mutedForeground)
                }
            }

            #if os(iOS)
            // Keyboard Shortcuts (for iPad users with external keyboards)
            Section {
                NavigationLink {
                    KeyboardShortcutsListView()
                } label: {
                    Label("Keyboard Shortcuts", systemImage: "keyboard")
                }
            }
            #endif
        }
        #if os(iOS)
        .listStyle(.insetGrouped)
        #endif
    }
}

#if os(iOS)
/// List view of keyboard shortcuts for iOS Settings
struct KeyboardShortcutsListView: View {
    private let shortcuts: [(section: String, items: [(key: String, description: String)])] = [
        ("Navigation", [
            ("j / ↓", "Move down / Next"),
            ("k / ↑", "Move up / Previous"),
            ("Enter", "Open selected"),
            ("Escape", "Go back / Close"),
        ]),
        ("Actions", [
            ("e", "Archive"),
            ("s", "Toggle star"),
            ("u", "Toggle read/unread"),
            ("#", "Move to trash"),
        ]),
        ("Go To", [
            ("g then i", "Go to Inbox"),
            ("g then s", "Go to Starred"),
            ("g then t", "Go to Sent"),
            ("g then d", "Go to Drafts"),
            ("g then a", "Go to All Mail"),
        ]),
        ("Search", [
            ("/ or ⌘K", "Focus search"),
            ("Escape", "Clear search"),
        ])
    ]

    var body: some View {
        List {
            ForEach(shortcuts, id: \.section) { section in
                Section(section.section) {
                    ForEach(section.items, id: \.key) { item in
                        HStack {
                            Text(item.key)
                                .font(.system(.body, design: .monospaced))
                                .foregroundColor(OrionTheme.primary)
                            Spacer()
                            Text(item.description)
                                .foregroundColor(OrionTheme.mutedForeground)
                        }
                    }
                }
            }
        }
        .navigationTitle("Keyboard Shortcuts")
        .navigationBarTitleDisplayMode(.inline)
    }
}
#endif
