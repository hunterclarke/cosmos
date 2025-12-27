import SwiftUI
import Combine

/// Search box with debounced input and keyboard shortcut
struct SearchBox: View {
    @Binding var query: String
    @Binding var isSearching: Bool
    @Binding var isEditing: Bool

    @State private var localQuery: String = ""
    @State private var isActive: Bool = false
    @FocusState private var textFieldFocused: Bool

    // Debounce timer
    @State private var debounceTask: Task<Void, Never>? = nil

    var body: some View {
        HStack(spacing: OrionTheme.spacing2) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12))
                .foregroundColor(OrionTheme.mutedForeground)

            if isActive {
                // Active: show real TextField
                TextField("Search emails...", text: $localQuery)
                    .textFieldStyle(.plain)
                    .font(.system(size: OrionTheme.textSm))
                    .foregroundColor(OrionTheme.foreground)
                    .focused($textFieldFocused)
                    .onSubmit {
                        commitSearch()
                    }
                    .onChange(of: localQuery) { _, newValue in
                        debounceSearch(newValue)
                    }
                    .onKeyPress(.escape) {
                        handleEscapeKey()
                        return .handled
                    }
                    .onAppear {
                        textFieldFocused = true
                    }
            } else {
                // Inactive: show placeholder text (not a TextField)
                Text("Search emails...")
                    .font(.system(size: OrionTheme.textSm))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            // Clear button
            if !localQuery.isEmpty {
                Button {
                    clearSearch()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 12))
                        .foregroundColor(OrionTheme.mutedForeground)
                }
                .buttonStyle(.plain)
            }

            // Keyboard shortcut hint
            if !isActive && localQuery.isEmpty {
                Text("/")
                    .font(.system(size: OrionTheme.textXs, weight: .medium))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(OrionTheme.border)
                    .cornerRadius(4)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            activateSearch()
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .padding(.vertical, OrionTheme.spacing2)
        .frame(width: OrionTheme.searchBoxWidth)
        .background(OrionTheme.secondary)
        .cornerRadius(6)
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(isActive ? OrionTheme.primary : OrionTheme.border, lineWidth: 1)
        )
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            activateSearch()
        }
    }

    private func activateSearch() {
        isActive = true
        isEditing = true
        isSearching = true
    }

    private func debounceSearch(_ value: String) {
        debounceTask?.cancel()

        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 150_000_000) // 150ms debounce

            guard !Task.isCancelled else { return }

            await MainActor.run {
                if !value.isEmpty {
                    query = value
                }
            }
        }
    }

    private func commitSearch() {
        debounceTask?.cancel()
        query = localQuery
        // Unfocus so user can navigate results with keyboard
        if !localQuery.isEmpty {
            isActive = false
            isEditing = false
            textFieldFocused = false
            // isSearching stays true so results are shown
        }
    }

    private func clearSearch() {
        localQuery = ""
        query = ""
        isSearching = false
        isEditing = false
        isActive = false
        textFieldFocused = false
    }

    private func handleEscapeKey() {
        if !localQuery.isEmpty {
            // First escape: clear the search text
            localQuery = ""
            query = ""
        } else {
            // Second escape: deactivate and exit search mode
            isActive = false
            isEditing = false
            isSearching = false
            textFieldFocused = false
        }
    }
}

extension Notification.Name {
    static let focusSearch = Notification.Name("focusSearch")
}

// MARK: - Preview

#Preview {
    VStack {
        SearchBox(query: .constant(""), isSearching: .constant(false), isEditing: .constant(false))
        SearchBox(query: .constant("test query"), isSearching: .constant(true), isEditing: .constant(true))
    }
    .padding()
    .background(OrionTheme.background)
}
