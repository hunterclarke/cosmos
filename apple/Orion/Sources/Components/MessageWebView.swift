import SwiftUI
import WebKit

/// WKWebView wrapper for rendering HTML email content with dark theme
/// Supports both macOS and iOS platforms
struct MessageWebView {
    let html: String

    /// Wraps raw HTML content with dark theme CSS styling
    private func wrapHtml(_ content: String) -> String {
        """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                * {
                    box-sizing: border-box;
                }
                body {
                    background-color: #1e1e1e;
                    color: #e0e0e0;
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
                    font-size: 14px;
                    line-height: 1.5;
                    margin: 0;
                    padding: 0;
                    word-wrap: break-word;
                    overflow-wrap: break-word;
                }
                a {
                    color: #58a6ff;
                    text-decoration: none;
                }
                a:hover {
                    text-decoration: underline;
                }
                blockquote {
                    border-left: 2px solid #444;
                    padding-left: 12px;
                    margin: 8px 0;
                    color: #888;
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

    /// Creates a configured WKWebView
    private func createWebView() -> WKWebView {
        let configuration = WKWebViewConfiguration()

        // Disable JavaScript for security
        configuration.defaultWebpagePreferences.allowsContentJavaScript = false

        // Enable data detection for links, dates, addresses
        #if os(iOS)
        configuration.dataDetectorTypes = [.link, .phoneNumber, .calendarEvent, .address]
        #endif

        let webView = WKWebView(frame: .zero, configuration: configuration)

        #if os(macOS)
        // Make background transparent on macOS
        webView.setValue(false, forKey: "drawsBackground")
        #else
        // Make background transparent on iOS
        webView.isOpaque = false
        webView.backgroundColor = .clear
        webView.scrollView.backgroundColor = .clear
        #endif

        return webView
    }

    /// Loads HTML content into the web view
    private func loadContent(in webView: WKWebView) {
        let wrappedHtml = wrapHtml(html)
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
