import Foundation

/// Exit status and captured output of one `codexmux` invocation.
struct CommandResult {
    let status: Int32
    let stdout: String
    let stderr: String

    var succeeded: Bool { status == 0 }

    /// Non-blank stdout lines, for commands that print one value per line.
    /// Standard error is excluded so diagnostics never become values.
    var stdoutLines: [String] {
        stdout.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    /// The end of the command's output, for a failure alert.
    var failureDetail: String {
        Self.tail(of: stdout + "\n" + stderr)
    }

    /// The last `maxLines` non-blank lines of `output`, trimmed and capped at
    /// about `maxCharacters` characters (the start is cut with an ellipsis).
    static func tail(of output: String, maxLines: Int = 6, maxCharacters: Int = 400) -> String {
        let lines = output.split(whereSeparator: \.isNewline)
            .filter { !$0.allSatisfy(\.isWhitespace) }
        let text = lines.suffix(maxLines)
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count > maxCharacters else { return text }
        let kept = text.suffix(maxCharacters - 1)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return "…" + kept
    }
}

/// Runs one executable to completion and captures its output. Both pipes are
/// drained concurrently so neither can fill up and stall the process.
struct CommandRunner {
    let executableURL: URL
    let environment: [String: String]

    /// Run synchronously (background queues only). A `timeout` terminates a
    /// call that stops responding and reports it as a failure.
    func run(_ arguments: [String], timeout: TimeInterval? = nil) -> CommandResult {
        let process = Process()
        process.executableURL = executableURL
        process.arguments = arguments
        process.environment = environment
        process.standardInput = FileHandle.nullDevice
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        do {
            try process.run()
        } catch {
            return CommandResult(
                status: -1,
                stdout: "",
                stderr: "\(executableURL.path) could not be started: \(error.localizedDescription)"
            )
        }

        let timedOut = Box(false)
        let watchdog = DispatchWorkItem {
            guard process.isRunning else { return }
            timedOut.value = true
            process.terminate()
        }
        if let timeout {
            DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout, execute: watchdog)
        }
        let stderrData = Box(Data())
        let stderrRead = DispatchGroup()
        stderrRead.enter()
        DispatchQueue.global(qos: .utility).async {
            stderrData.value = stderrPipe.fileHandleForReading.readDataToEndOfFile()
            stderrRead.leave()
        }
        let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        stderrRead.wait()
        process.waitUntilExit()
        watchdog.cancel()

        var errorOutput = String(decoding: stderrData.value, as: UTF8.self)
        if timedOut.value, let timeout {
            errorOutput += "\n\(executableURL.lastPathComponent) \(arguments.joined(separator: " ")) timed out after \(Int(timeout)) seconds"
        }
        return CommandResult(
            status: process.terminationStatus,
            stdout: String(decoding: stdoutData, as: UTF8.self),
            stderr: errorOutput
        )
    }
}

/// Mutable storage shared with work running on another queue.
private final class Box<Value> {
    var value: Value

    init(_ value: Value) {
        self.value = value
    }
}
