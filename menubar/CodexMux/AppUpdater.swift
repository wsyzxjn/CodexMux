import AppKit
import CryptoKit
import Foundation

struct AppRelease {
    let version: String
    let archiveName: String
    let archiveURL: URL
    let checksumsURL: URL
}

struct PreparedAppUpdate {
    let stagedAppURL: URL
    let stagingRootURL: URL
}

enum AppUpdaterError: Error {
    case invalidResponse
    case invalidRelease
    case missingAsset(String)
    case checksumMismatch
    case invalidBundle
    case unsupportedInstallLocation
    case commandFailed(String)
}

final class AppUpdater {
    private let repository: String
    private let session: URLSession

    init(repository: String) {
        self.repository = repository
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 30
        configuration.timeoutIntervalForResource = 300
        configuration.httpAdditionalHeaders = [
            "Accept": "application/vnd.github+json",
            "User-Agent": "CodexMux-Updater",
            "X-GitHub-Api-Version": "2022-11-28",
        ]
        session = URLSession(configuration: configuration)
    }

    func checkForUpdate(
        currentVersion: String,
        completion: @escaping (Result<AppRelease?, Error>) -> Void
    ) {
        guard let apiURL = URL(
            string: "https://api.github.com/repos/\(repository)/releases/latest"
        ) else {
            completion(.failure(AppUpdaterError.invalidRelease))
            return
        }

        session.dataTask(with: apiURL) { data, response, error in
            if let error {
                completion(.failure(error))
                return
            }
            guard let http = response as? HTTPURLResponse,
                  http.statusCode == 200,
                  let data,
                  data.count <= 2_000_000,
                  let payload = try? JSONDecoder().decode(GitHubRelease.self, from: data),
                  !payload.draft,
                  !payload.prerelease else {
                completion(.failure(AppUpdaterError.invalidResponse))
                return
            }

            let version = payload.tagName.hasPrefix("v")
                ? String(payload.tagName.dropFirst())
                : payload.tagName
            guard Self.isNewer(version, than: currentVersion) else {
                completion(.success(nil))
                return
            }

            let architecture = Self.releaseArchitecture
            let archiveName = "CodexMux-\(version)-macos-\(architecture).zip"
            guard let archiveURL = payload.asset(named: archiveName),
                  let checksumsURL = payload.asset(named: "SHA256SUMS"),
                  Self.isTrustedDownloadURL(archiveURL),
                  Self.isTrustedDownloadURL(checksumsURL) else {
                completion(.failure(AppUpdaterError.missingAsset(archiveName)))
                return
            }
            completion(.success(AppRelease(
                version: version,
                archiveName: archiveName,
                archiveURL: archiveURL,
                checksumsURL: checksumsURL
            )))
        }.resume()
    }

    func prepareUpdate(
        _ release: AppRelease,
        completion: @escaping (Result<PreparedAppUpdate, Error>) -> Void
    ) {
        Task.detached { [session] in
            do {
                let prepared = try await Self.downloadAndValidate(release, session: session)
                completion(.success(prepared))
            } catch {
                completion(.failure(error))
            }
        }
    }

    func installAndRelaunch(_ prepared: PreparedAppUpdate) throws {
        let currentAppURL = Bundle.main.bundleURL.standardizedFileURL
        guard currentAppURL.pathExtension == "app",
              FileManager.default.isWritableFile(
                atPath: currentAppURL.deletingLastPathComponent().path
              ) else {
            throw AppUpdaterError.unsupportedInstallLocation
        }

        let backupURL = currentAppURL.deletingLastPathComponent().appendingPathComponent(
            "CodexMux.app.backup-\(Int(Date().timeIntervalSince1970))"
        )
        let script = """
        while /bin/kill -0 "$PARENT_PID" 2>/dev/null; do /bin/sleep 0.1; done
        if ! /bin/mv "$CURRENT_APP" "$BACKUP_APP"; then exit 1; fi
        if /usr/bin/ditto "$STAGED_APP" "$CURRENT_APP" && \
           /usr/bin/codesign --verify --deep --strict "$CURRENT_APP"; then
          /usr/bin/open "$CURRENT_APP"
          /bin/rm -rf "$STAGING_ROOT"
          exit 0
        fi
        /bin/rm -rf "$CURRENT_APP"
        /bin/mv "$BACKUP_APP" "$CURRENT_APP"
        /usr/bin/open "$CURRENT_APP"
        exit 1
        """
        let helper = Process()
        helper.executableURL = URL(fileURLWithPath: "/bin/sh")
        helper.arguments = ["-c", script]
        helper.environment = [
            "PARENT_PID": String(ProcessInfo.processInfo.processIdentifier),
            "CURRENT_APP": currentAppURL.path,
            "BACKUP_APP": backupURL.path,
            "STAGED_APP": prepared.stagedAppURL.path,
            "STAGING_ROOT": prepared.stagingRootURL.path,
        ]
        try helper.run()
        NSApp.terminate(nil)
    }

    static func isNewer(_ candidate: String, than current: String) -> Bool {
        guard let candidateParts = versionParts(candidate),
              let currentParts = versionParts(current) else { return false }
        let count = max(candidateParts.count, currentParts.count)
        for index in 0..<count {
            let left = index < candidateParts.count ? candidateParts[index] : 0
            let right = index < currentParts.count ? currentParts[index] : 0
            if left != right { return left > right }
        }
        return false
    }

    static func expectedChecksum(
        for fileName: String,
        in manifest: String
    ) -> String? {
        for line in manifest.split(whereSeparator: \.isNewline) {
            let fields = line.split(whereSeparator: \.isWhitespace)
            guard fields.count == 2 else { continue }
            var listedName = fields[1].hasPrefix("*")
                ? fields[1].dropFirst()
                : fields[1][...]
            if listedName.hasPrefix("./") {
                listedName = listedName.dropFirst(2)
            }
            if listedName == fileName,
               fields[0].count == 64,
               fields[0].allSatisfy({ $0.isHexDigit }) {
                return fields[0].lowercased()
            }
        }
        return nil
    }

    private static var releaseArchitecture: String {
        #if arch(arm64)
        return "arm64"
        #elseif arch(x86_64)
        return "x86_64"
        #else
        return "unsupported"
        #endif
    }

    private static func versionParts(_ value: String) -> [Int]? {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingPrefix("v")
        let core = normalized.split(separator: "-", maxSplits: 1)[0]
        let parts = core.split(separator: ".").map { Int($0) }
        guard !parts.isEmpty, parts.allSatisfy({ $0 != nil }) else { return nil }
        return parts.compactMap { $0 }
    }

    private static func isTrustedDownloadURL(_ url: URL) -> Bool {
        url.scheme == "https" && url.host == "github.com"
    }

    private static func downloadAndValidate(
        _ release: AppRelease,
        session: URLSession
    ) async throws -> PreparedAppUpdate {
        let (manifestData, manifestResponse) = try await session.data(from: release.checksumsURL)
        guard let http = manifestResponse as? HTTPURLResponse,
              http.statusCode == 200,
              manifestData.count <= 1_000_000,
              let manifest = String(data: manifestData, encoding: .utf8),
              let expected = expectedChecksum(for: release.archiveName, in: manifest) else {
            throw AppUpdaterError.invalidResponse
        }

        let (downloadedURL, archiveResponse) = try await session.download(from: release.archiveURL)
        guard let http = archiveResponse as? HTTPURLResponse,
              http.statusCode == 200 else {
            throw AppUpdaterError.invalidResponse
        }

        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "CodexMux-update-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        do {
            let archive = root.appendingPathComponent(release.archiveName)
            try FileManager.default.moveItem(at: downloadedURL, to: archive)
            guard try sha256(of: archive) == expected else {
                throw AppUpdaterError.checksumMismatch
            }

            let unpacked = root.appendingPathComponent("unpacked", isDirectory: true)
            try FileManager.default.createDirectory(at: unpacked, withIntermediateDirectories: true)
            try run("/usr/bin/ditto", ["-x", "-k", archive.path, unpacked.path])

            let app = unpacked.appendingPathComponent("CodexMux.app", isDirectory: true)
            guard let bundle = Bundle(url: app),
                  bundle.bundleIdentifier == "dev.codexmux.menubar",
                  bundle.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
                    == release.version,
                  let cliURL = bundle.url(forResource: "codexmux", withExtension: nil) else {
                throw AppUpdaterError.invalidBundle
            }
            let cliVersion = try commandOutput(cliURL.path, ["--version"])
            guard cliVersion.split(whereSeparator: \.isWhitespace).last
                    == Substring(release.version) else {
                throw AppUpdaterError.invalidBundle
            }
            try run("/usr/bin/codesign", ["--verify", "--deep", "--strict", app.path])
            return PreparedAppUpdate(stagedAppURL: app, stagingRootURL: root)
        } catch {
            try? FileManager.default.removeItem(at: root)
            throw error
        }
    }

    private static func sha256(of url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while true {
            let data = try handle.read(upToCount: 1_048_576) ?? Data()
            if data.isEmpty { break }
            hasher.update(data: data)
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func run(_ executable: String, _ arguments: [String]) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw AppUpdaterError.commandFailed(executable)
        }
    }

    private static func commandOutput(_ executable: String, _ arguments: [String]) throws -> String {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw AppUpdaterError.commandFailed(executable)
        }
        return String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

private struct GitHubRelease: Decodable {
    struct Asset: Decodable {
        let name: String
        let browserDownloadURL: URL

        enum CodingKeys: String, CodingKey {
            case name
            case browserDownloadURL = "browser_download_url"
        }
    }

    let tagName: String
    let draft: Bool
    let prerelease: Bool
    let assets: [Asset]

    enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case draft
        case prerelease
        case assets
    }

    func asset(named name: String) -> URL? {
        assets.first { $0.name == name }?.browserDownloadURL
    }
}
