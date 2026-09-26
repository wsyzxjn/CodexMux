import XCTest
@testable import CodexMux

final class MenubarStateTests: XCTestCase {
    private let complete = """
    {
      "version": "0.3.1",
      "proxy": "running",
      "cpa": {
        "local": true,
        "installed": true,
        "running": true,
        "version": "7.2.147",
        "rollback_available": true,
        "autostart": true
      },
      "catalog": {
        "advertise_ultra": true,
        "unify_comp_hash": false,
        "models": ["gpt-5.6-sol", "cpa/claude-fable-5"]
      },
      "profiles": {
        "active": "home",
        "saved": [
          {"name": "home", "base_url": "http://127.0.0.1:8317/v1"},
          {"name": "office", "base_url": "https://cpa.example.com/v1"}
        ]
      },
      "review_override": "claude-fable-5",
      "image_override": "gpt-image-2",
      "search": {
        "mode": "enabled",
        "backend_model": "cpa/claude-fable-5",
        "default_backend_model": "gpt-5.6-sol"
      },
      "search_capabilities": {
        "gpt-5.6-sol": "verified",
        "cpa/claude-fable-5": "supported",
        "cpa/snake_case_model": "unsupported",
        "cpa/unchecked": "unknown",
        "cpa/broken": "error"
      },
      "errors": [],
      "added_in_a_later_cli": {"ignored": true}
    }
    """

    func testDecodesCompleteStateAndIgnoresUnknownFields() throws {
        let state = try MenubarState.decode(Data(complete.utf8))

        XCTAssertEqual(state.version, "0.3.1")
        XCTAssertEqual(state.proxy, .running)
        XCTAssertEqual(state.cpa, MenubarState.CPA(
            local: true,
            installed: true,
            running: true,
            version: "7.2.147",
            rollbackAvailable: true,
            autostart: true
        ))
        XCTAssertEqual(state.catalog, MenubarState.Catalog(
            advertiseUltra: true,
            unifyCompHash: false,
            models: ["gpt-5.6-sol", "cpa/claude-fable-5"]
        ))
        XCTAssertEqual(state.profiles.active, "home")
        XCTAssertEqual(state.profiles.saved, [
            MenubarState.Profile(name: "home", baseURL: "http://127.0.0.1:8317/v1"),
            MenubarState.Profile(name: "office", baseURL: "https://cpa.example.com/v1"),
        ])
        XCTAssertEqual(state.reviewOverride, "claude-fable-5")
        XCTAssertEqual(state.imageOverride, "gpt-image-2")
        XCTAssertEqual(state.search, MenubarState.Search(
            mode: .enabled,
            backendModel: "cpa/claude-fable-5",
            defaultBackendModel: "gpt-5.6-sol"
        ))
        // Capability keys are exact slugs; snake case must not be rewritten.
        XCTAssertEqual(state.searchCapabilities, [
            "gpt-5.6-sol": .verified,
            "cpa/claude-fable-5": .supported,
            "cpa/snake_case_model": .unsupported,
            "cpa/unchecked": .unknown,
            "cpa/broken": .error,
        ])
        XCTAssertEqual(state.errors, [])
    }

    func testDecodesNullFields() throws {
        let json = """
        {
          "version": "0.3.1",
          "proxy": "not_installed",
          "cpa": {
            "local": false,
            "installed": false,
            "running": false,
            "version": null,
            "rollback_available": false,
            "autostart": false
          },
          "catalog": {"advertise_ultra": false, "unify_comp_hash": true, "models": []},
          "profiles": {"active": null, "saved": []},
          "review_override": null,
          "image_override": null,
          "search": {"mode": "default", "backend_model": null, "default_backend_model": null},
          "search_capabilities": {},
          "errors": []
        }
        """
        let state = try MenubarState.decode(Data(json.utf8))

        XCTAssertEqual(state.proxy, .notInstalled)
        XCTAssertFalse(state.cpa.local)
        XCTAssertNil(state.cpa.version)
        XCTAssertNil(state.profiles.active)
        XCTAssertEqual(state.profiles.saved, [])
        XCTAssertNil(state.reviewOverride)
        XCTAssertNil(state.imageOverride)
        XCTAssertEqual(state.search, MenubarState.Search(
            mode: .default,
            backendModel: nil,
            defaultBackendModel: nil
        ))
        XCTAssertEqual(state.searchCapabilities, [:])
    }

    func testDecodesReportedErrors() throws {
        let json = complete
            .replacingOccurrences(of: "\"proxy\": \"running\"", with: "\"proxy\": \"idle\"")
            .replacingOccurrences(
                of: "\"errors\": []",
                with: "\"errors\": [\"model catalog snapshot has not been built yet\", \"second\"]"
            )
        let state = try MenubarState.decode(Data(json.utf8))

        XCTAssertEqual(state.proxy, .idle)
        XCTAssertEqual(state.errors.first, "model catalog snapshot has not been built yet")
        XCTAssertEqual(state.errors.count, 2)
    }

    func testDescribesOutputThatIsNotTheContract() {
        func failure(_ json: String) -> String? {
            do {
                _ = try MenubarState.decode(Data(json.utf8))
                return nil
            } catch {
                return MenubarState.describe(error)
            }
        }

        XCTAssertEqual(
            failure(complete.replacingOccurrences(of: "\"proxy\": \"running\"", with: "\"proxy\": \"stopped\"")),
            "invalid value for proxy"
        )
        XCTAssertEqual(
            failure(complete.replacingOccurrences(of: "\"local\": true,", with: "")),
            "missing field cpa.local"
        )
        XCTAssertEqual(
            failure(complete.replacingOccurrences(of: "\"cpa/broken\": \"error\"", with: "\"cpa/broken\": \"pending\"")),
            "invalid value for search_capabilities.cpa/broken"
        )
        // A log line on stdout breaks the one-object contract.
        XCTAssertEqual(failure("WARN ignoring unreadable catalog snapshot\n" + complete), "output is not valid JSON")
    }
}
