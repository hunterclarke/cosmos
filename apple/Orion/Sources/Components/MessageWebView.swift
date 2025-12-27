import SwiftUI
import WebKit

/// WKWebView wrapper for rendering HTML email content with dark theme
/// Supports both macOS and iOS platforms
struct MessageWebView {
    let html: String

    #if os(iOS)
    @Binding var contentHeight: CGFloat

    init(html: String, contentHeight: Binding<CGFloat>) {
        self.html = html
        self._contentHeight = contentHeight
    }
    #else
    init(html: String) {
        self.html = html
    }
    #endif

    /// Wraps raw HTML content with dark theme CSS styling
    private func wrapHtml(_ content: String, for platform: Platform) -> String {
        let fontSize = platform == .iOS ? "17px" : "14px"
        let padding = platform == .iOS ? "12px" : "0"

        // iOS: Don't constrain viewport - let content render at natural size
        // User can scroll horizontally for wide emails or pinch to zoom
        // macOS: Use device width for responsive rendering
        let viewportMeta = platform == .iOS
            ? ""  // No viewport meta - let WKWebView handle it naturally
            : "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"

        return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            \(viewportMeta)
            <style>
                /* Base styles for the wrapper - namespaced to avoid conflicts */
                html, body {
                    margin: 0;
                    padding: 0;
                    width: 100%;
                    min-height: 100%;
                    background-color: #1e1e1e;
                    -webkit-text-size-adjust: 100%;
                }

                /* Orion mail wrapper - all our styles are scoped here */
                .orion-mail-content {
                    color: #e0e0e0;
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
                    font-size: \(fontSize);
                    line-height: 1.6;
                    padding: \(padding);
                    word-wrap: break-word;
                    overflow-wrap: break-word;
                    overflow-x: auto;
                }

                /* Default link color if email doesn't specify */
                .orion-mail-content a {
                    color: #58a6ff;
                }

                /* Scrollbar styling */
                ::-webkit-scrollbar {
                    width: 8px;
                    height: 8px;
                }
                ::-webkit-scrollbar-track {
                    background: #1e1e1e;
                }
                ::-webkit-scrollbar-thumb {
                    background: #444;
                    border-radius: 4px;
                }
                ::-webkit-scrollbar-thumb:hover {
                    background: #555;
                }
            </style>
        </head>
        <body>
            <div class="orion-mail-content">\(content)</div>
        </body>
        </html>
        """
    }

    enum Platform {
        case macOS
        case iOS
    }

    /// Creates a configured WKWebView
    private func createWebView() -> WKWebView {
        let configuration = WKWebViewConfiguration()

        // Disable JavaScript for security (not needed for email viewing)
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false

        #if os(iOS)
        // Enable data detection for links, dates, addresses
        configuration.dataDetectorTypes = [.link, .phoneNumber, .calendarEvent, .address]
        #endif

        let webView = WKWebView(frame: .zero, configuration: configuration)

        #if os(macOS)
        // Make background transparent on macOS
        webView.setValue(false, forKey: "drawsBackground")
        #else
        // Configure iOS WebView
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear

        // Enable scrolling so wide emails can scroll horizontally
        // This allows emails designed for 600px to render at full size
        webView.scrollView.isScrollEnabled = true
        webView.scrollView.bounces = true

        // Prevent automatic content scaling
        webView.scrollView.minimumZoomScale = 1.0
        webView.scrollView.maximumZoomScale = 3.0
        #endif

        return webView
    }

    /// Loads HTML content into the web view
    private func loadContent(in webView: WKWebView) {
        #if os(iOS)
        let platform = Platform.iOS
        #else
        let platform = Platform.macOS
        #endif
        let wrappedHtml = wrapHtml(html, for: platform)
        webView.loadHTMLString(wrappedHtml, baseURL: nil)
    }
}

// MARK: - macOS Implementation

#if os(macOS)
extension MessageWebView: NSViewRepresentable {
    func makeNSView(context: Context) -> WKWebView {
        let webView = createWebView()
        loadContent(in: webView)
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        // Only reload if content changed
        if context.coordinator.lastHtml != html {
            context.coordinator.lastHtml = html
            loadContent(in: webView)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    class Coordinator {
        var lastHtml: String = ""
    }
}
#endif

// MARK: - iOS Implementation

#if os(iOS)
extension MessageWebView: UIViewRepresentable {
    func makeUIView(context: Context) -> WKWebView {
        let webView = createWebView()
        webView.navigationDelegate = context.coordinator
        context.coordinator.heightBinding = _contentHeight
        loadContent(in: webView)
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {
        // Only reload if content changed
        if context.coordinator.lastHtml != html {
            context.coordinator.lastHtml = html
            loadContent(in: webView)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    class Coordinator: NSObject, WKNavigationDelegate {
        var lastHtml: String = ""
        var heightBinding: Binding<CGFloat>?

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            // Measure content height after page loads
            webView.evaluateJavaScript("document.body.scrollHeight") { [weak self] result, error in
                if let height = result as? CGFloat, height > 0 {
                    DispatchQueue.main.async {
                        self?.heightBinding?.wrappedValue = height
                    }
                }
            }
        }
    }
}
#endif
