import SwiftUI
import AuthenticationServices
import Security

/// OAuth authentication service using ASWebAuthenticationSession
///
/// Handles Google OAuth flow for Gmail API access.
@MainActor
class AuthService: NSObject, ObservableObject, ASWebAuthenticationPresentationContextProviding {
    // MARK: - Configuration

    /// OAuth client ID (from Google Cloud Console)
    private(set) var clientId: String = ""

    /// OAuth client secret
    private(set) var clientSecret: String = ""

    /// OAuth redirect URI - uses reverse client ID format for iOS compatibility
    /// Google automatically allows this scheme without manual registration
    private var redirectUri: String {
        // Extract the numeric part from client ID (e.g., "123456789-abc.apps.googleusercontent.com" -> "123456789-abc")
        let clientIdPrefix = clientId.replacingOccurrences(of: ".apps.googleusercontent.com", with: "")
        return "com.googleusercontent.apps.\(clientIdPrefix):/oauthredirect"
    }

    /// Callback URL scheme for ASWebAuthenticationSession
    private var callbackScheme: String {
        let clientIdPrefix = clientId.replacingOccurrences(of: ".apps.googleusercontent.com", with: "")
        return "com.googleusercontent.apps.\(clientIdPrefix)"
    }

    /// Gmail API scope
    private let scope = "https://www.googleapis.com/auth/gmail.modify"

    // MARK: - Published State

    @Published var isAuthenticating: Bool = false
    @Published var isConfigured: Bool = false
    @Published var error: String? = nil

    // MARK: - Initialization

    override init() {
        super.init()
        loadCredentialsFromBundle()
    }

    // MARK: - Configuration

    /// Configure OAuth credentials manually
    func configure(clientId: String, clientSecret: String) {
        self.clientId = clientId
        self.clientSecret = clientSecret
        self.isConfigured = !clientId.isEmpty && !clientSecret.isEmpty
    }

    /// Load credentials from app bundle (embedded at build time via xcconfig)
    func loadCredentialsFromBundle() {
        guard let clientId = Bundle.main.object(forInfoDictionaryKey: "GoogleClientID") as? String,
              let clientSecret = Bundle.main.object(forInfoDictionaryKey: "GoogleClientSecret") as? String,
              !clientId.isEmpty, !clientSecret.isEmpty else {
            OrionLogger.auth.warning("OAuth credentials not configured in build settings")
            OrionLogger.auth.info("Run ./script/setup-credentials to configure")
            return
        }

        self.clientId = clientId
        self.clientSecret = clientSecret
        self.isConfigured = true
        OrionLogger.auth.info("Loaded OAuth credentials from bundle")
    }

    // MARK: - Authentication

    /// Start OAuth flow
    func authenticate() async throws -> OAuthTokens {
        isAuthenticating = true
        defer { isAuthenticating = false }

        // Generate PKCE code verifier and challenge
        let codeVerifier = generateCodeVerifier()
        let codeChallenge = generateCodeChallenge(from: codeVerifier)

        // Build authorization URL
        let authUrl = buildAuthorizationUrl(codeChallenge: codeChallenge)

        // Present web authentication session
        let scheme = callbackScheme
        let callbackUrl = try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<URL, Error>) in
            let session = ASWebAuthenticationSession(
                url: authUrl,
                callbackURLScheme: scheme
            ) { url, error in
                if let error = error {
                    continuation.resume(throwing: error)
                } else if let url = url {
                    continuation.resume(returning: url)
                } else {
                    continuation.resume(throwing: AuthError.unknown)
                }
            }

            session.presentationContextProvider = self
            session.prefersEphemeralWebBrowserSession = false

            if !session.start() {
                continuation.resume(throwing: AuthError.sessionStartFailed)
            }
        }

        // Extract authorization code from callback
        guard let code = extractCode(from: callbackUrl) else {
            throw AuthError.noAuthorizationCode
        }

        // Exchange code for tokens
        return try await exchangeCodeForTokens(code: code, codeVerifier: codeVerifier)
    }

    /// Refresh an expired access token
    func refreshToken(_ refreshToken: String) async throws -> OAuthTokens {
        var request = URLRequest(url: URL(string: "https://oauth2.googleapis.com/token")!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")

        let body = [
            "client_id": clientId,
            "client_secret": clientSecret,
            "refresh_token": refreshToken,
            "grant_type": "refresh_token"
        ]

        request.httpBody = body
            .map { "\($0.key)=\($0.value)" }
            .joined(separator: "&")
            .data(using: .utf8)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse,
              httpResponse.statusCode == 200 else {
            throw AuthError.tokenExchangeFailed
        }

        let tokenResponse = try JSONDecoder().decode(TokenResponse.self, from: data)

        return OAuthTokens(
            accessToken: tokenResponse.accessToken,
            refreshToken: tokenResponse.refreshToken ?? refreshToken,
            expiresAt: Date().addingTimeInterval(TimeInterval(tokenResponse.expiresIn ?? 3600))
        )
    }

    // MARK: - Token Storage

    /// Save tokens to Keychain
    func saveTokens(_ tokens: OAuthTokens, for accountId: Int64) throws {
        let key = "orion.oauth.\(accountId)"
        let data = try JSONEncoder().encode(tokens)

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key,
            kSecValueData as String: data
        ]

        // Delete existing item
        SecItemDelete(query as CFDictionary)

        // Add new item
        let status = SecItemAdd(query as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw AuthError.keychainError(status)
        }
    }

    /// Load tokens from Keychain
    func loadTokens(for accountId: Int64) throws -> OAuthTokens? {
        let key = "orion.oauth.\(accountId)"

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key,
            kSecReturnData as String: true
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)

        guard status == errSecSuccess,
              let data = result as? Data else {
            if status == errSecItemNotFound {
                return nil
            }
            throw AuthError.keychainError(status)
        }

        return try JSONDecoder().decode(OAuthTokens.self, from: data)
    }

    /// Delete tokens from Keychain
    func deleteTokens(for accountId: Int64) throws {
        let key = "orion.oauth.\(accountId)"

        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrAccount as String: key
        ]

        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw AuthError.keychainError(status)
        }
    }

    /// Get valid tokens for an account, refreshing if expired
    func getValidTokens(for accountId: Int64) async throws -> OAuthTokens {
        guard var tokens = try loadTokens(for: accountId) else {
            throw AuthError.noTokensFound
        }

        // Refresh if expired or about to expire (within 5 minutes)
        if tokens.expiresAt < Date().addingTimeInterval(300) {
            guard let refreshToken = tokens.refreshToken else {
                throw AuthError.noRefreshToken
            }
            tokens = try await self.refreshToken(refreshToken)
            try saveTokens(tokens, for: accountId)
        }

        return tokens
    }

    // MARK: - ASWebAuthenticationPresentationContextProviding

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        #if os(macOS)
        return NSApplication.shared.windows.first ?? NSWindow()
        #else
        return UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow } ?? UIWindow()
        #endif
    }

    // MARK: - Private Helpers

    private func generateCodeVerifier() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        return Data(bytes).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private func generateCodeChallenge(from verifier: String) -> String {
        guard let data = verifier.data(using: .utf8) else { return "" }
        var hash = [UInt8](repeating: 0, count: 32)
        _ = data.withUnsafeBytes { bytes in
            CC_SHA256(bytes.baseAddress, CC_LONG(data.count), &hash)
        }
        return Data(hash).base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    private func buildAuthorizationUrl(codeChallenge: String) -> URL {
        var components = URLComponents(string: "https://accounts.google.com/o/oauth2/v2/auth")!
        components.queryItems = [
            URLQueryItem(name: "client_id", value: clientId),
            URLQueryItem(name: "redirect_uri", value: redirectUri),
            URLQueryItem(name: "response_type", value: "code"),
            URLQueryItem(name: "scope", value: scope),
            URLQueryItem(name: "code_challenge", value: codeChallenge),
            URLQueryItem(name: "code_challenge_method", value: "S256"),
            URLQueryItem(name: "access_type", value: "offline"),
            URLQueryItem(name: "prompt", value: "consent")
        ]
        return components.url!
    }

    private func extractCode(from url: URL) -> String? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
              let code = components.queryItems?.first(where: { $0.name == "code" })?.value else {
            return nil
        }
        return code
    }

    private func exchangeCodeForTokens(code: String, codeVerifier: String) async throws -> OAuthTokens {
        var request = URLRequest(url: URL(string: "https://oauth2.googleapis.com/token")!)
        request.httpMethod = "POST"
        request.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")

        let body = [
            "client_id": clientId,
            "client_secret": clientSecret,
            "code": code,
            "code_verifier": codeVerifier,
            "grant_type": "authorization_code",
            "redirect_uri": redirectUri
        ]

        request.httpBody = body
            .map { "\($0.key)=\($0.value)" }
            .joined(separator: "&")
            .data(using: .utf8)

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse,
              httpResponse.statusCode == 200 else {
            throw AuthError.tokenExchangeFailed
        }

        let tokenResponse = try JSONDecoder().decode(TokenResponse.self, from: data)

        return OAuthTokens(
            accessToken: tokenResponse.accessToken,
            refreshToken: tokenResponse.refreshToken,
            expiresAt: Date().addingTimeInterval(TimeInterval(tokenResponse.expiresIn ?? 3600))
        )
    }
}

// MARK: - Supporting Types

struct OAuthTokens: Codable {
    let accessToken: String
    let refreshToken: String?
    let expiresAt: Date

    var isExpired: Bool {
        expiresAt < Date()
    }

    /// Create token JSON for Rust FFI
    func toTokenJson() -> String {
        let expiresAtTimestamp = Int64(expiresAt.timeIntervalSince1970)
        let dict: [String: Any] = [
            "access_token": accessToken,
            "refresh_token": refreshToken as Any,
            "expires_at": expiresAtTimestamp
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: dict),
              let json = String(data: data, encoding: .utf8) else {
            return "{}"
        }
        return json
    }
}

private struct TokenResponse: Codable {
    let accessToken: String
    let refreshToken: String?
    let expiresIn: Int?
    let tokenType: String

    enum CodingKeys: String, CodingKey {
        case accessToken = "access_token"
        case refreshToken = "refresh_token"
        case expiresIn = "expires_in"
        case tokenType = "token_type"
    }
}

enum AuthError: Error, LocalizedError {
    case sessionStartFailed
    case noAuthorizationCode
    case tokenExchangeFailed
    case keychainError(OSStatus)
    case noTokensFound
    case noRefreshToken
    case notConfigured
    case unknown

    var errorDescription: String? {
        switch self {
        case .sessionStartFailed:
            return "Failed to start authentication session"
        case .noAuthorizationCode:
            return "No authorization code received"
        case .tokenExchangeFailed:
            return "Failed to exchange code for tokens"
        case .keychainError(let status):
            return "Keychain error: \(status)"
        case .noTokensFound:
            return "No tokens found for account"
        case .noRefreshToken:
            return "No refresh token available"
        case .notConfigured:
            return "OAuth not configured. Please add google-credentials.json"
        case .unknown:
            return "Unknown error"
        }
    }
}

// CommonCrypto import for SHA256
import CommonCrypto
