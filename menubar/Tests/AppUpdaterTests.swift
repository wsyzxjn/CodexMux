import XCTest
@testable import CodexMux

final class AppUpdaterTests: XCTestCase {
    func testVersionComparisonUsesNumericSegments() {
        XCTAssertTrue(AppUpdater.isNewer("0.2.1", than: "0.2.0"))
        XCTAssertTrue(AppUpdater.isNewer("v1.0.0", than: "0.99.9"))
        XCTAssertFalse(AppUpdater.isNewer("0.2.0", than: "0.2"))
        XCTAssertFalse(AppUpdater.isNewer("0.1.99", than: "0.2.0"))
        XCTAssertFalse(AppUpdater.isNewer("invalid", than: "0.2.0"))
    }

    func testChecksumManifestAcceptsReleaseWorkflowPaths() {
        let hash = String(repeating: "a", count: 64)
        let manifest = "\(hash)  ./CodexMux-0.3.0-macos-arm64.zip\n"
        XCTAssertEqual(
            AppUpdater.expectedChecksum(
                for: "CodexMux-0.3.0-macos-arm64.zip",
                in: manifest
            ),
            hash
        )
    }

    func testChecksumManifestRejectsWrongOrMalformedEntries() {
        XCTAssertNil(AppUpdater.expectedChecksum(
            for: "CodexMux-0.3.0-macos-arm64.zip",
            in: "abcd  ./CodexMux-0.3.0-macos-arm64.zip\n"
        ))
        XCTAssertNil(AppUpdater.expectedChecksum(
            for: "CodexMux-0.3.0-macos-arm64.zip",
            in: "\(String(repeating: "b", count: 64))  other.zip\n"
        ))
    }
}
