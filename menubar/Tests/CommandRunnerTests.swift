import XCTest
@testable import CodexMux

final class CommandRunnerTests: XCTestCase {
    private let shell = CommandRunner(
        executableURL: URL(fileURLWithPath: "/bin/sh"),
        environment: ["PATH": "/usr/bin:/bin"]
    )

    func testFailureDetailKeepsTheLastNonBlankLinesOfBothStreams() {
        let result = CommandResult(
            status: 1,
            stdout: (1...8).map { "step \($0)" }.joined(separator: "\n") + "\n\n",
            stderr: "Error: failed to enable the managed Codex configuration\n\nCaused by:\n    permission denied  \n\n"
        )
        XCTAssertEqual(
            result.failureDetail,
            "step 6\nstep 7\nstep 8\nError: failed to enable the managed Codex configuration\nCaused by:\n    permission denied"
        )
    }

    func testFailureDetailIsCappedFromTheStart() {
        let detail = CommandResult.tail(of: "Error: " + String(repeating: "x", count: 1_000) + " end")
        XCTAssertEqual(detail.count, 400)
        XCTAssertTrue(detail.hasPrefix("…"))
        XCTAssertTrue(detail.hasSuffix("x end"))
    }

    func testDrainsBothPipesWhenStandardErrorOutgrowsThePipeBuffer() {
        let result = shell.run(["-c", "head -c 200000 /dev/zero | tr '\\0' e >&2; echo done"], timeout: 10)
        XCTAssertTrue(result.succeeded)
        XCTAssertEqual(result.stdoutLines, ["done"])
        XCTAssertEqual(result.stderr.count, 200_000)
    }

    func testTimeoutTerminatesACommandThatStopsResponding() {
        let result = shell.run(["-c", "exec sleep 30"], timeout: 0.5)
        XCTAssertFalse(result.succeeded)
        XCTAssertTrue(result.failureDetail.contains("timed out"))
    }
}
