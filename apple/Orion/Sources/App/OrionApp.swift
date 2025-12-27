import SwiftUI

/// Main entry point for the Orion mail application
@main
struct OrionApp: App {
    /// Shared mail bridge that wraps the Rust MailService
    @StateObject private var mailBridge = MailBridge()

    /// Shared authentication service
    @StateObject private var authService = AuthService()

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

/// Placeholder for settings view
struct SettingsView: View {
    var body: some View {
        Text("Settings")
            .padding()
    }
}
