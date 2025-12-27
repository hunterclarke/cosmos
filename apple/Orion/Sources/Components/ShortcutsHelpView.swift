import SwiftUI

/// Modal overlay showing keyboard shortcuts
/// Adapts to iOS with a sheet presentation and responsive layout
struct ShortcutsHelpView: View {
    @Binding var isPresented: Bool
    @Environment(\.horizontalSizeClass) var horizontalSizeClass

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
            ("g then #", "Go to Trash"),
        ]),
        ("Search", [
            ("/ or ⌘K", "Focus search"),
            ("Escape", "Clear search"),
        ]),
        ("Help", [
            ("?", "Show this help"),
        ])
    ]

    var body: some View {
        #if os(iOS)
        // On iOS, use a sheet presentation
        Color.clear
            .sheet(isPresented: $isPresented) {
                NavigationStack {
                    shortcutsContent
                        .navigationTitle("Keyboard Shortcuts")
                        .navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button("Done") {
                                    isPresented = false
                                }
                            }
                        }
                }
                .presentationDetents([.medium, .large])
            }
        #else
        // On macOS, use the modal overlay
        macOSModalView
        #endif
    }

    #if os(macOS)
    private var macOSModalView: some View {
        ZStack {
            // Backdrop
            OrionTheme.modalBackdrop
                .ignoresSafeArea()
                .onTapGesture {
                    isPresented = false
                }

            // Modal content
            VStack(spacing: 0) {
                // Header
                HStack {
                    Text("Keyboard Shortcuts")
                        .font(.system(size: OrionTheme.textLg, weight: .semibold))
                        .foregroundColor(OrionTheme.foreground)

                    Spacer()

                    Button {
                        isPresented = false
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 14))
                            .foregroundColor(OrionTheme.mutedForeground)
                    }
                    .buttonStyle(.plain)
                    .keyboardShortcut(.escape, modifiers: [])
                }
                .padding(OrionTheme.spacing4)

                Divider()
                    .background(OrionTheme.border)

                shortcutsContent
            }
            .frame(width: 600, height: 500)
            .background(OrionTheme.background)
            .cornerRadius(12)
            .overlay(
                RoundedRectangle(cornerRadius: 12)
                    .stroke(OrionTheme.border, lineWidth: 1)
            )
            .shadow(color: .black.opacity(0.3), radius: 20, x: 0, y: 10)
        }
    }
    #endif

    private var shortcutsContent: some View {
        ScrollView {
            LazyVGrid(columns: gridColumns, spacing: OrionTheme.spacing4) {
                ForEach(shortcuts, id: \.section) { section in
                    ShortcutSection(title: section.section, items: section.items)
                }
            }
            .padding(OrionTheme.spacing4)
        }
        .background(OrionTheme.background)
    }

    private var gridColumns: [GridItem] {
        #if os(iOS)
        if horizontalSizeClass == .compact {
            return [GridItem(.flexible())]
        } else {
            return [GridItem(.flexible()), GridItem(.flexible())]
        }
        #else
        return [GridItem(.flexible()), GridItem(.flexible())]
        #endif
    }
}

// MARK: - Shortcut Section

struct ShortcutSection: View {
    let title: String
    let items: [(key: String, description: String)]

    var body: some View {
        VStack(alignment: .leading, spacing: OrionTheme.spacing2) {
            Text(title)
                .font(.system(size: OrionTheme.textSm, weight: .semibold))
                .foregroundColor(OrionTheme.foreground)
                .textCase(.uppercase)

            VStack(alignment: .leading, spacing: OrionTheme.spacing1) {
                ForEach(items, id: \.key) { item in
                    ShortcutRow(key: item.key, description: item.description)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

// MARK: - Shortcut Row

struct ShortcutRow: View {
    let key: String
    let description: String

    var body: some View {
        HStack(spacing: OrionTheme.spacing2) {
            // Key badge(s)
            HStack(spacing: 4) {
                ForEach(key.components(separatedBy: " "), id: \.self) { part in
                    if part == "then" {
                        Text(part)
                            .font(.system(size: OrionTheme.textXs))
                            .foregroundColor(OrionTheme.mutedForeground)
                    } else {
                        Text(part)
                            .font(.system(size: OrionTheme.textXs, weight: .medium, design: .monospaced))
                            .foregroundColor(OrionTheme.foreground)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 3)
                            .background(OrionTheme.secondary)
                            .cornerRadius(4)
                            .overlay(
                                RoundedRectangle(cornerRadius: 4)
                                    .stroke(OrionTheme.border, lineWidth: 1)
                            )
                    }
                }
            }
            .frame(minWidth: 80, alignment: .leading)

            Text(description)
                .font(.system(size: OrionTheme.textSm))
                .foregroundColor(OrionTheme.mutedForeground)

            Spacer()
        }
    }
}

// MARK: - Preview

#Preview {
    ZStack {
        OrionTheme.background
            .ignoresSafeArea()

        ShortcutsHelpView(isPresented: .constant(true))
    }
}
