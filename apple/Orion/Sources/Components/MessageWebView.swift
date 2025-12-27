import SwiftUI
import WebKit

/// WKWebView wrapper for rendering HTML email content with dark theme
/// Supports both macOS and iOS platforms
struct MessageWebView {
    let html: String

    /// Wraps raw HTML content with dark theme CSS styling
    /// iOS uses a fixed viewport width to ensure emails render at readable size
    private func wrapHtml(_ content: String, for platform: Platform) -> String {
        let fontSize = platform == .iOS ? "16px" : "14px"
        let padding = platform == .iOS ? "16px" : "0"

        // iOS: Use a fixed viewport width (typical email width) so content renders at readable size
        // The WKWebView will scale this to fit the screen
        // macOS: Use device width
        let viewport = platform == .iOS
            ? "width=600, initial-scale=1.0, user-scalable=yes"
            : "width=device-width, initial-scale=1.0"

        return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="\(viewport)">
            <style>
                * {
                    box-sizing: border-box;
                }
                html, body {
                    margin: 0;
                    padding: 0;
                    width: 100%;
                    min-height: 100%;
                }
                body {
                    background-color: #1e1e1e;
                    color: #e0e0e0;
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
                    font-size: \(fontSize);
                    line-height: 1.6;
                    padding: \(padding);
                    word-wrap: break-word;
                    overflow-wrap: break-word;
                    -webkit-text-size-adjust: 100%;
                }
                a {
                    color: #58a6ff;
                    text-decoration: none;
                }
                a:hover {
                    text-decoration: underline;
                }
                blockquote {
                    border-left: 3px solid #444;
                    padding-left: 12px;
                    margin: 12px 0;
                    color: #999;
                }
                img {
                    max-width: 100%;
                    height: auto;
                }
                pre, code {
                    background-color: #2d2d2d;
                    padding: 4px;
                    border-radius: 4px;
                    overflow-x: auto;
                    font-family: 'SF Mono', Monaco, Consolas, monospace;
                    font-size: 13px;
                }
                pre {
                    padding: 12px;
                    margin: 8px 0;
                }
                table {
                    border-collapse: collapse;
                    max-width: 100%;
                }
                td, th {
                    padding: 8px;
                    border: 1px solid #444;
                }
                hr {
                    border: none;
                    border-top: 1px solid #444;
                    margin: 16px 0;
                }
                /* Hide scrollbar but allow scrolling */
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
            \(content)
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

        // Disable scrolling within the WebView (parent ScrollView handles it)
        webView.scrollView.isScrollEnabled = false
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

    class Coordinator {
        var lastHtml: String = ""
    }
}
#endif
