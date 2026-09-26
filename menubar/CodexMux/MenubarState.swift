import Foundation

/// Local state printed by `codexmux menubar-state` as one JSON object on
/// standard output.
///
/// The CLI owns this format. Unknown fields are ignored so the CLI can add
/// fields without breaking the menu; a missing or mistyped known field is a
/// decoding error that the menu reports instead of guessing.
struct MenubarState: Decodable, Equatable {
    enum Proxy: String, Decodable {
        case running
        case idle
        case notInstalled = "not_installed"
    }

    struct CPA: Decodable, Equatable {
        /// Whether the configured CPA endpoint is on this machine. The managed
        /// CLIProxyAPI controls (start, stop, update, …) only apply when it is.
        let local: Bool
        let installed: Bool
        let running: Bool
        let version: String?
        let rollbackAvailable: Bool
        let autostart: Bool

        private enum CodingKeys: String, CodingKey {
            case local, installed, running, version, autostart
            case rollbackAvailable = "rollback_available"
        }
    }

    struct Catalog: Decodable, Equatable {
        let advertiseUltra: Bool
        let unifyCompHash: Bool
        let models: [String]

        private enum CodingKeys: String, CodingKey {
            case models
            case advertiseUltra = "advertise_ultra"
            case unifyCompHash = "unify_comp_hash"
        }
    }

    struct Profile: Decodable, Equatable {
        let name: String
        let baseURL: String

        private enum CodingKeys: String, CodingKey {
            case name
            case baseURL = "base_url"
        }
    }

    struct Profiles: Decodable, Equatable {
        let active: String?
        let saved: [Profile]
    }

    struct Search: Decodable, Equatable {
        enum Mode: String, Decodable {
            case `default`
            case enabled
            case disabled
        }

        let mode: Mode
        let backendModel: String?
        let defaultBackendModel: String?

        private enum CodingKeys: String, CodingKey {
            case mode
            case backendModel = "backend_model"
            case defaultBackendModel = "default_backend_model"
        }
    }

    enum SearchCapability: String, Decodable {
        case verified
        case supported
        case unsupported
        case unknown
        case error
    }

    let version: String
    let proxy: Proxy
    let cpa: CPA
    let catalog: Catalog
    let profiles: Profiles
    let reviewOverride: String?
    let imageOverride: String?
    let search: Search
    /// Keyed by exact catalog slug. Explicit coding keys (rather than a
    /// snake-case key strategy) keep these slugs byte-for-byte.
    let searchCapabilities: [String: SearchCapability]
    let errors: [String]

    private enum CodingKeys: String, CodingKey {
        case version, proxy, cpa, catalog, profiles, search, errors
        case reviewOverride = "review_override"
        case imageOverride = "image_override"
        case searchCapabilities = "search_capabilities"
    }

    static func decode(_ data: Data) throws -> MenubarState {
        try JSONDecoder().decode(MenubarState.self, from: data)
    }

    /// A short, single-line reason `decode` failed, for the menu's error row.
    static func describe(_ error: Error) -> String {
        guard let error = error as? DecodingError else {
            return error.localizedDescription
        }
        switch error {
        case .keyNotFound(let key, let context):
            return "missing field \(path(context.codingPath + [key]))"
        case .valueNotFound(_, let context):
            return "null field \(path(context.codingPath))"
        case .typeMismatch(_, let context):
            return "unexpected type for \(path(context.codingPath))"
        case .dataCorrupted(let context):
            return context.codingPath.isEmpty
                ? "output is not valid JSON"
                : "invalid value for \(path(context.codingPath))"
        @unknown default:
            return "output could not be decoded"
        }
    }

    private static func path(_ keys: [CodingKey]) -> String {
        keys.reduce(into: "") { text, key in
            if let index = key.intValue {
                text += "[\(index)]"
            } else {
                text += text.isEmpty ? key.stringValue : ".\(key.stringValue)"
            }
        }
    }
}
