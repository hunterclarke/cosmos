import SwiftUI
import Combine

/// Search box with debounced input and keyboard shortcut
struct SearchBox: View {
    @Binding var query: String
    @Binding var isSearching: Bool

    @State private var localQuery: String = ""
    @State private var isFocused: Bool = false
    @FocusState private var textFieldFocused: Bool

    // Debounce timer
    @State private var debounceTask: Task<Void, Never>? = nil

    var body: some View {
        HStack(spacing: OrionTheme.spacing2) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12))
                .foregroundColor(OrionTheme.mutedForeground)

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
            if !isFocused && localQuery.isEmpty {
                Text("/")
                    .font(.system(size: OrionTheme.textXs, weight: .medium))
                    .foregroundColor(OrionTheme.mutedForeground)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(OrionTheme.border)
                    .cornerRadius(4)
            }
        }
        .padding(.horizontal, OrionTheme.spacing3)
        .padding(.vertical, OrionTheme.spacing2)
        .frame(width: OrionTheme.searchBoxWidth)
        .background(OrionTheme.secondary)
        .cornerRadius(6)
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(isFocused ? OrionTheme.primary : OrionTheme.border, lineWidth: 1)
        )
        .onChange(of: textFieldFocused) { _, focused in
            isFocused = focused
            if focused {
                isSearching = true
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .focusSearch)) { _ in
            textFieldFocused = true
        }
    }

    private func debounceSearch(_ value: String) {
        debounceTask?.cancel()

        debounceTask = Task {
            try? await Task.sleep(nanoseconds: 150_000_000) // 150ms debounce

            guard !Task.isCancelled else { return }

            await MainActor.run {
                if !value.isEmpty {
                    query = value
                    isSearching = true
                }
            }
        }
    }

    private func commitSearch() {
        debounceTask?.cancel()
        query = localQuery
        isSearching = !localQuery.isEmpty
    }

    private func clearSearch() {
        localQuery = ""
        query = ""
        isSearching = false
        textFieldFocused = false
    }
}

extension Notification.Name {
    static let focusSearch = Notification.Name("focusSearch")
}

// MARK: - Preview

#Preview {
    VStack {
        SearchBox(query: .constant(""), isSearching: .constant(false))
        SearchBox(query: .constant("test query"), isSearching: .constant(true))
    }
    .padding()
    .background(OrionTheme.background)
}
