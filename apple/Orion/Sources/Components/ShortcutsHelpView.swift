import SwiftUI

/// Modal overlay showing keyboard shortcuts
struct ShortcutsHelpView: View {
    @Binding var isPresented: Bool

    private let shortcuts: [(section: String, items: [(key: String, description: String)])] = [
        ("Navigation", [
            ("j / ↓", "Move to next thread"),
            ("k / ↑", "Move to previous thread"),
            ("Enter / o", "Open selected thread"),
            ("Escape / u", "Go back to list"),
            ("g then i", "Go to Inbox"),
            ("g then s", "Go to Starred"),
            ("g then d", "Go to Drafts"),
            ("g then t", "Go to Sent"),
        ]),
        ("Actions", [
            ("e", "Archive"),
            ("s", "Star/Unstar"),
            ("Shift+I", "Mark as read"),
            ("Shift+U", "Mark as unread"),
            ("#", "Delete"),
            ("r", "Reply"),
            ("a", "Reply all"),
            ("f", "Forward"),
        ]),
        ("Search", [
            ("/", "Focus search box"),
            ("Escape", "Clear search"),
        ]),
        ("Other", [
            ("?", "Show this help"),
            ("Cmd+,", "Open settings"),
        ])
    ]

    var body: some View {
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

                // Shortcuts grid
                ScrollView {
                    LazyVGrid(columns: [
                        GridItem(.flexible()),
                        GridItem(.flexible())
                    ], spacing: OrionTheme.spacing4) {
                        ForEach(shortcuts, id: \.section) { section in
                            ShortcutSection(title: section.section, items: section.items)
                        }
                    }
                    .padding(OrionTheme.spacing4)
                }
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
