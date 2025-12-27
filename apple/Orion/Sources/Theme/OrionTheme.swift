import SwiftUI

/// Theme colors and styling for Orion
///
/// Based on gpui-component's dark theme, matching docs/ui.md spec
enum OrionTheme {
    // MARK: - Base Colors

    /// Main content background
    static let background = Color(hex: "1e1e1e")

    /// Primary text color
    static let foreground = Color(hex: "e0e0e0")

    /// Secondary/subdued text
    static let mutedForeground = Color(hex: "808080")

    /// Borders and dividers
    static let border = Color(hex: "3c3c3c")

    // MARK: - Secondary Colors

    /// Sidebar background, keyboard hints
    static let secondary = Color(hex: "252526")

    /// Text on secondary backgrounds
    static let secondaryForeground = Color(hex: "cccccc")

    // MARK: - List Colors

    /// List item background (default)
    static let list = Color(hex: "1e1e1e")

    /// Selected list item background
    static let listActive = Color(hex: "094771")

    /// Hovered list item background
    static let listHover = Color(hex: "2a2d2e")

    /// Left accent border on selected items
    static let listActiveBorder = Color(hex: "0078d4")

    // MARK: - Accent Colors

    /// Primary accent (unread dots, avatars)
    static let primary = Color(hex: "0078d4")

    /// Text on primary backgrounds
    static let primaryForeground = Color(hex: "ffffff")

    // MARK: - Error Colors

    /// Error state background
    static let danger = Color(hex: "f44336")

    /// Error state text
    static let dangerForeground = Color(hex: "ffffff")

    // MARK: - Search Highlight

    /// Yellow highlight for search matches (40% opacity)
    static let searchHighlight = Color(hue: 50/360, saturation: 0.9, brightness: 0.5).opacity(0.4)

    // MARK: - Modal

    /// Semi-transparent overlay for modals
    static let modalBackdrop = Color.black.opacity(0.5)

    // MARK: - Spacing

    /// Base spacing unit (4px)
    static let spacingUnit: CGFloat = 4

    /// Common spacing values
    static let spacing1: CGFloat = 4
    static let spacing2: CGFloat = 8
    static let spacing3: CGFloat = 12
    static let spacing4: CGFloat = 16
    static let spacing8: CGFloat = 32

    // MARK: - Typography

    /// Font sizes matching text_xs, text_sm, text_lg
    static let textXs: CGFloat = 11
    static let textSm: CGFloat = 13
    static let textLg: CGFloat = 17

    // MARK: - Component Sizes

    /// Sidebar width
    static let sidebarWidth: CGFloat = 240

    /// Thread item height
    static let threadItemHeight: CGFloat = 40

    /// Search box width
    static let searchBoxWidth: CGFloat = 280

    /// Avatar size
    static let avatarSize: CGFloat = 20

    /// Unread indicator dot size
    static let unreadDotSize: CGFloat = 6

    /// Sender column width
    static let senderColumnWidth: CGFloat = 180

    /// Account column width (unified view)
    static let accountColumnWidth: CGFloat = 140
}

// MARK: - Color Extension for Hex

extension Color {
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let a, r, g, b: UInt64
        switch hex.count {
        case 3: // RGB (12-bit)
            (a, r, g, b) = (255, (int >> 8) * 17, (int >> 4 & 0xF) * 17, (int & 0xF) * 17)
        case 6: // RGB (24-bit)
            (a, r, g, b) = (255, int >> 16, int >> 8 & 0xFF, int & 0xFF)
        case 8: // ARGB (32-bit)
            (a, r, g, b) = (int >> 24, int >> 16 & 0xFF, int >> 8 & 0xFF, int & 0xFF)
        default:
            (a, r, g, b) = (255, 0, 0, 0)
        }
        self.init(
            .sRGB,
            red: Double(r) / 255,
            green: Double(g) / 255,
            blue: Double(b) / 255,
            opacity: Double(a) / 255
        )
    }
}
