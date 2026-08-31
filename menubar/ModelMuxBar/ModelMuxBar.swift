import Cocoa

/// User-facing strings for every language ModelMuxBar supports.
struct L10n {
    let modelmuxStatusRunning: String
    let modelmuxStatusStopped: String
    let cpaStatusRunning: String
    let cpaStatusStopped: String
    let controlsRunning: String
    let controlsStopped: String
    let restartModelMux: String
    let stopModelMux: String
    let startCPA: String
    let stopCPA: String
    let openLogs: String
    let profiles: String
    let profileNoProfiles: String
    let reviewModel: String
    let reviewDefault: String
    let profileActiveSuffix: String
    let language: String
    let quit: String
    let restartFailed: String
    let stopFailed: String
    let startCPAFailed: String
    let stopCPAFailed: String
    let reviewSetFailed: String
    let tokenSpeed: String
    let quitDialogTitle: String
    let quitDialogBody: String
    let quitDialogConfirm: String
    let quitDialogCancel: String
    let alertOK: String

    static let english = L10n(
        modelmuxStatusRunning: "ModelMux: running",
        modelmuxStatusStopped: "ModelMux: stopped",
        cpaStatusRunning: "CPA service: running",
        cpaStatusStopped: "CPA service: stopped",
        controlsRunning: "Services ▸",
        controlsStopped: "Services… ▸",
        restartModelMux: "Restart ModelMux",
        stopModelMux: "Stop ModelMux",
        startCPA: "Start CPA service",
        stopCPA: "Stop CPA service",
        openLogs: "Open Logs Folder",
        profiles: "CPA Profiles",
        profileNoProfiles: "No saved profiles",
        reviewModel: "Review Model",
        reviewDefault: "Default (official first)",
        profileActiveSuffix: "  ✓",
        language: "Language",
        quit: "Quit ModelMuxBar",
        restartFailed: "Failed to restart ModelMux. See logs.",
        stopFailed: "Failed to stop ModelMux. See logs.",
        startCPAFailed: "Failed to start the CPA service. See logs.",
        stopCPAFailed: "Failed to stop the CPA service. See logs.",
        reviewSetFailed: "Failed to set the review model. See logs.",
        tokenSpeed: "Token Speed",
        quitDialogTitle: "Quit ModelMuxBar?",
        quitDialogBody: "This stops the ModelMux proxy and the CPA service, and restores the Codex configuration.",
        quitDialogConfirm: "Quit and Stop Services",
        quitDialogCancel: "Cancel",
        alertOK: "OK"
    )

    static let chinese = L10n(
        modelmuxStatusRunning: "ModelMux：运行中",
        modelmuxStatusStopped: "ModelMux：已停止",
        cpaStatusRunning: "CPA 服务：运行中",
        cpaStatusStopped: "CPA 服务：已停止",
        controlsRunning: "服务控制 ▸",
        controlsStopped: "服务控制… ▸",
        restartModelMux: "重启 ModelMux",
        stopModelMux: "停止 ModelMux",
        startCPA: "启动 CPA 服务",
        stopCPA: "停止 CPA 服务",
        openLogs: "打开日志文件夹",
        profiles: "CPA 配置",
        profileNoProfiles: "（暂无保存的配置）",
        reviewModel: "审批模型",
        reviewDefault: "默认（官方优先）",
        profileActiveSuffix: "  ✓",
        language: "语言",
        quit: "退出 ModelMuxBar",
        restartFailed: "重启 ModelMux 失败，请查看日志。",
        stopFailed: "停止 ModelMux 失败，请查看日志。",
        startCPAFailed: "启动 CPA 服务失败，请查看日志。",
        stopCPAFailed: "停止 CPA 服务失败，请查看日志。",
        reviewSetFailed: "设置审批模型失败，请查看日志。",
        tokenSpeed: "Token 速度",
        quitDialogTitle: "退出 ModelMuxBar？",
        quitDialogBody: "将停止 ModelMux 代理与 CPA 服务，并还原 Codex 配置。",
        quitDialogConfirm: "退出并停止服务",
        quitDialogCancel: "取消",
        alertOK: "好"
    )

    /// True when this is the Chinese localization.
    var isChinese: Bool { quit == "退出 ModelMuxBar" }

    func profileSwitchFailed(_ name: String) -> String {
        if isChinese {
            return "切换到配置 \(name) 失败，未做更改。请查看日志。"
        }
        return "Failed to switch to profile \(name). No changes made. See logs."
    }

    static func forLanguage(_ language: Language) -> L10n {
        switch language {
        case .systemPreferred:
            let isChinese = Locale.preferredLanguages.first.map {
                $0.hasPrefix("zh")
            } ?? false
            return isChinese ? .chinese : .english
        case .english:
            return .english
        case .chinese:
            return .chinese
        }
    }
}

enum Language: String, CaseIterable {
    case systemPreferred = "system"
    case english = "en"
    case chinese = "zh"

    var displayName: String {
        switch self {
        case .systemPreferred: return "Auto (System)"
        case .english: return "English"
        case .chinese: return "中文"
        }
    }
}

struct TelemetrySnapshot: Decodable {
    let current: TurnTelemetry?
    let lastCompleted: TurnTelemetry?

    enum CodingKeys: String, CodingKey {
        case current
        case lastCompleted = "last_completed"
    }
}

struct TurnTelemetry: Decodable {
    let tokensPerSecond: Double?
    let exact: Bool

    enum CodingKeys: String, CodingKey {
        case tokensPerSecond = "tokens_per_second"
        case exact
    }
}

/// Menu bar controller for ModelMux: service status, start/stop, and log access.
///
/// ModelMuxBar only manages ModelMux's own lifecycle (and its bundled CPA
/// service). Model selection stays in the Codex client; this app deliberately
/// never switches models.
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var statusItem: NSStatusItem!
    private var menu: NSMenu!
    private var timer: Timer?
    private var telemetryTimer: Timer?
    private var telemetryRequestInFlight = false
    private var telemetry: TelemetrySnapshot?
    private var tokenSpeedMenu: NSMenu!
    private var tokenSpeedValueItem: NSMenuItem!

    private let modelmuxURL = URL(fileURLWithPath: NSString(
        string: "~/.local/bin/modelmux"
    ).expandingTildeInPath)
    private let modelmuxHome: String = {
        let configured = ProcessInfo.processInfo.environment["MODELMUX_HOME"]
        let path = configured.flatMap { $0.isEmpty ? nil : $0 }
            ?? "~/Library/Application Support/ModelMux"
        return NSString(string: path).expandingTildeInPath
    }()
    /// Codex config path; must match the LaunchAgent plist so install/uninstall
    /// and serve manage the same file.
    private let codexConfig = NSString(
        string: "/Volumes/AmatsukaM/.agent-data/codex/config.toml"
    ).expandingTildeInPath
    private let languageDefaultsKey = "language"
    private let proxyPort: Int = {
        guard let configured = ProcessInfo.processInfo.environment["MODELMUX_PROXY_PORT"],
              let port = Int(configured), (1...65_535).contains(port)
        else { return 48_682 }
        return port
    }()
    private var proxyReachable = false
    private var cpaRunning = false
    private var activeProfile: String?
    private var savedProfiles: [(name: String, baseURL: String)] = []
    private var reviewOverride: String?
    private var cpaModels: [String] = []

    private var language: Language {
        Language(rawValue: UserDefaults.standard.string(forKey: languageDefaultsKey) ?? "") ?? .systemPreferred
    }

    private var l10n: L10n { L10n.forLanguage(language) }

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let image = NSImage(systemSymbolName: "arrow.triangle.branch",
                               accessibilityDescription: "ModelMux") {
            statusItem.button?.image = image
        } else {
            statusItem.button?.title = "MM"
        }
        menu = NSMenu()
        menu.autoenablesItems = false
        statusItem.menu = menu
        statusItem.button?.toolTip = "ModelMux"
        rebuildMenu()
        refreshStatus()
        refreshTelemetry()
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            self?.refreshStatus()
        }
        let telemetryTimer = Timer(timeInterval: 0.75, repeats: true) { [weak self] _ in
            self?.refreshTelemetry()
        }
        self.telemetryTimer = telemetryTimer
        RunLoop.main.add(telemetryTimer, forMode: .common)
    }

    func applicationWillTerminate(_ notification: Notification) {
        timer?.invalidate()
        telemetryTimer?.invalidate()
    }

    // MARK: - Status

    private func refreshStatus() {
        let group = DispatchGroup()
        group.enter()
        checkProxy { [weak self] reachable in
            self?.proxyReachable = reachable
            group.leave()
        }
        group.enter()
        checkCPA { [weak self] running in
            self?.cpaRunning = running
            group.leave()
        }
        group.enter()
        loadProfiles { [weak self] active, saved in
            self?.activeProfile = active
            self?.savedProfiles = saved
            group.leave()
        }
        group.enter()
        loadReviewState { [weak self] overrideSlug, models in
            self?.reviewOverride = overrideSlug
            self?.cpaModels = models
            group.leave()
        }
        group.notify(queue: .main) { [weak self] in
            guard let self else { return }
            self.updateIcon()
            self.rebuildMenu()
        }
    }

    private func refreshTelemetry() {
        guard !telemetryRequestInFlight else { return }
        let token = credentialToken(named: "proxy_token")
        guard !token.isEmpty else {
            telemetry = nil
            updateTokenSpeedMenu()
            return
        }
        telemetryRequestInFlight = true
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(proxyPort)/telemetry")!)
        request.timeoutInterval = 2
        request.setValue(token, forHTTPHeaderField: "x-modelmux-token")
        URLSession.shared.dataTask(with: request) { [weak self] data, response, _ in
            let snapshot: TelemetrySnapshot? = {
                guard (response as? HTTPURLResponse)?.statusCode == 200,
                      let data else { return nil }
                return try? JSONDecoder().decode(TelemetrySnapshot.self, from: data)
            }()
            DispatchQueue.main.async {
                guard let self else { return }
                self.telemetryRequestInFlight = false
                if let snapshot {
                    self.telemetry = snapshot
                    self.proxyReachable = true
                    self.updateIcon()
                } else {
                    self.telemetry = nil
                }
                self.updateTokenSpeedMenu()
            }
        }.resume()
    }

    /// Fetch CPA model slugs from the local CPA instance (background queue only).
    private func fetchCpaModels() -> [String] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/curl")
        process.arguments = [
            "-s", "-m", "5",
            "http://127.0.0.1:8317/v1/models",
            "-H", "Authorization: Bearer \(cpaToken())",
        ]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return [] }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let list = object["data"] as? [[String: Any]]
        else { return [] }
        return list.compactMap { $0["id"] as? String }
    }

    /// Read the CPA client token from ModelMux credentials (never logged).
    private func cpaToken() -> String {
        credentialToken(named: "cpa_token")
    }

    private func credentialToken(named key: String) -> String {
        let path = modelmuxHome + "/credentials.json"
        guard let data = FileManager.default.contents(atPath: path),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let token = object[key] as? String
        else { return "" }
        return token
    }

    /// Load the review override and the CPA model list (background queue only).
    private func loadReviewState(
        _ completion: @escaping (String?, [String]) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let overrideOutput = self.captureModelmux(["cpa", "review-get"])
            let overrideSlug: String? = {
                let line = overrideOutput.split(separator: "\n").first { $0.hasPrefix("review override: ") }
                guard let line else { return nil }
                let value = line.dropFirst("review override: ".count)
                return value.hasPrefix("(none") ? nil : String(value)
            }()
            // Query the local CPA catalog directly for cpa/ model slugs.
            let models = self.fetchCpaModels()
            DispatchQueue.main.async { completion(overrideSlug, models) }
        }
    }

    /// Parse `modelmux cpa profile-list` output (background queue only).
    private func loadProfiles(_ completion: @escaping (String?, [(name: String, baseURL: String)]) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureModelmux(["cpa", "profile-list"])
            var active: String?
            var saved: [(name: String, baseURL: String)] = []
            for line in output.split(separator: "\n") {
                if line.hasPrefix("active: ") {
                    let value = line.dropFirst("active: ".count)
                    if value != "(none; using config.toml settings)" {
                        active = String(value)
                    }
                } else if line.hasPrefix("  ") {
                    // "  name — base-url"
                    let body = line.dropFirst(2)
                    if let separator = body.range(of: " \u{2014} ") {
                        saved.append((String(body[..<separator.lowerBound]),
                                      String(body[separator.upperBound...])))
                    }
                }
            }
            DispatchQueue.main.async { completion(active, saved) }
        }
    }

    private func checkProxy(_ completion: @escaping (Bool) -> Void) {
        var request = URLRequest(url: URL(string: "http://127.0.0.1:\(proxyPort)/health")!)
        request.timeoutInterval = 2
        let token = credentialToken(named: "proxy_token")
        if !token.isEmpty {
            request.setValue(token, forHTTPHeaderField: "x-modelmux-token")
        }
        URLSession.shared.dataTask(with: request) { _, response, _ in
            completion((response as? HTTPURLResponse)?.statusCode == 200)
        }.resume()
    }

    private func checkCPA(_ completion: @escaping (Bool) -> Void) {
        runModelmux(["cpa", "status"]) { output in
            completion(output.contains("service: running"))
        }
    }

    private func updateIcon() {
        let running = NSImage(systemSymbolName: "arrow.triangle.branch",
                              accessibilityDescription: "ModelMux")
        let stopped = NSImage(systemSymbolName: "arrow.triangle.branch.exclamationmark",
                              accessibilityDescription: "ModelMux stopped")
        if let image = proxyReachable ? running : stopped {
            statusItem.button?.title = ""
            statusItem.button?.image = image
        } else {
            statusItem.button?.image = nil
            statusItem.button?.title = proxyReachable ? "MM" : "MM!"
        }
    }

    // MARK: - Menu

    private func rebuildMenu() {
        let l10n = self.l10n
        menu.removeAllItems()

        // Status lines live in collapsible submenus so the top level stays short.
        let modelmuxStatusItem = NSMenuItem(
            title: proxyReachable ? l10n.modelmuxStatusRunning : l10n.modelmuxStatusStopped,
            action: nil, keyEquivalent: ""
        )
        modelmuxStatusItem.isEnabled = false
        menu.addItem(modelmuxStatusItem)

        let cpaStatusItem = NSMenuItem(
            title: cpaRunning ? l10n.cpaStatusRunning : l10n.cpaStatusStopped,
            action: nil, keyEquivalent: ""
        )
        cpaStatusItem.isEnabled = false
        menu.addItem(cpaStatusItem)

        let tokenSpeedItem = NSMenuItem(title: l10n.tokenSpeed, action: nil, keyEquivalent: "")
        tokenSpeedMenu = NSMenu()
        tokenSpeedMenu.autoenablesItems = false
        tokenSpeedMenu.delegate = self
        tokenSpeedValueItem = NSMenuItem(title: "— tok/s", action: nil, keyEquivalent: "")
        tokenSpeedValueItem.isEnabled = false
        tokenSpeedMenu.addItem(tokenSpeedValueItem)
        tokenSpeedItem.submenu = tokenSpeedMenu
        menu.addItem(tokenSpeedItem)
        updateTokenSpeedMenu()

        // Controls submenu: start/stop actions for both services.
        let controlsTitle = cpaRunning
            ? l10n.controlsRunning
            : l10n.controlsStopped
        let controlsItem = NSMenuItem(title: controlsTitle, action: nil, keyEquivalent: "")
        let controls = NSMenu()
        controls.autoenablesItems = false

        let restart = NSMenuItem(title: l10n.restartModelMux, action: #selector(restartProxy),
                                 keyEquivalent: "r")
        restart.target = self
        restart.isEnabled = true
        controls.addItem(restart)

        let stop = NSMenuItem(title: l10n.stopModelMux, action: #selector(stopProxy),
                              keyEquivalent: "s")
        stop.target = self
        stop.isEnabled = proxyReachable
        controls.addItem(stop)

        controls.addItem(.separator())

        let startCPAItem = NSMenuItem(title: l10n.startCPA, action: #selector(startCPA),
                                      keyEquivalent: "")
        startCPAItem.target = self
        startCPAItem.isEnabled = !cpaRunning
        controls.addItem(startCPAItem)

        let stopCPAItem = NSMenuItem(title: l10n.stopCPA, action: #selector(stopCPA),
                                     keyEquivalent: "")
        stopCPAItem.target = self
        stopCPAItem.isEnabled = cpaRunning
        controls.addItem(stopCPAItem)
        controlsItem.submenu = controls
        menu.addItem(controlsItem)

        // CPA profiles submenu: click a saved endpoint to validate and switch.
        let profilesItem = NSMenuItem(title: l10n.profiles, action: nil, keyEquivalent: "")
        let profilesMenu = NSMenu()
        profilesMenu.autoenablesItems = false
        if savedProfiles.isEmpty {
            let empty = NSMenuItem(title: l10n.profileNoProfiles, action: nil, keyEquivalent: "")
            empty.isEnabled = false
            profilesMenu.addItem(empty)
        } else {
            for profile in savedProfiles {
                let isActive = activeProfile == profile.name
                let title = isActive ? profile.name + l10n.profileActiveSuffix : profile.name
                let item = NSMenuItem(title: title,
                                      action: #selector(switchProfile(_:)),
                                      keyEquivalent: "")
                item.target = self
                item.representedObject = profile.name
                item.state = isActive ? .on : .off
                profilesMenu.addItem(item)
            }
        }
        profilesItem.submenu = profilesMenu
        menu.addItem(profilesItem)

        // Review model submenu: pick which CPA model handles codex-auto-review.
        let reviewItem = NSMenuItem(title: l10n.reviewModel, action: nil, keyEquivalent: "")
        let reviewMenu = NSMenu()
        reviewMenu.autoenablesItems = false

        let defaultItem = NSMenuItem(title: l10n.reviewDefault,
                                     action: #selector(selectReviewModel(_:)),
                                     keyEquivalent: "")
        defaultItem.target = self
        defaultItem.representedObject = ""
        defaultItem.state = reviewOverride == nil ? .on : .off
        reviewMenu.addItem(defaultItem)

        for slug in cpaModels.sorted() {
            let item = NSMenuItem(title: slug,
                                  action: #selector(selectReviewModel(_:)),
                                  keyEquivalent: "")
            item.target = self
            item.representedObject = slug
            item.state = reviewOverride == slug ? .on : .off
            reviewMenu.addItem(item)
        }
        reviewItem.submenu = reviewMenu
        menu.addItem(reviewItem)

        let logs = NSMenuItem(title: l10n.openLogs, action: #selector(openLogs),
                              keyEquivalent: "l")
        logs.target = self
        menu.addItem(logs)

        // Language submenu with the three options; checkmark marks the active one.
        let languageItem = NSMenuItem(title: l10n.language, action: nil, keyEquivalent: "")
        let languageMenu = NSMenu()
        for option in Language.allCases {
            let item = NSMenuItem(title: option.displayName,
                                  action: #selector(selectLanguage(_:)),
                                  keyEquivalent: "")
            item.target = self
            item.representedObject = option.rawValue
            item.state = option == language ? .on : .off
            languageMenu.addItem(item)
        }
        languageItem.submenu = languageMenu
        menu.addItem(languageItem)

        menu.addItem(.separator())
        let quit = NSMenuItem(title: l10n.quit, action: #selector(confirmQuit(_:)),
                              keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    // MARK: - Actions

    private func updateTokenSpeedMenu() {
        guard tokenSpeedValueItem != nil else { return }
        let displayedTurn = telemetry?.current ?? telemetry?.lastCompleted

        if let turn = displayedTurn, let speed = turn.tokensPerSecond {
            let prefix = turn.exact ? "" : "≈ "
            tokenSpeedValueItem.title = prefix + String(format: "%.1f tok/s", speed)
        } else {
            tokenSpeedValueItem.title = "— tok/s"
        }
    }

    func menuWillOpen(_ menu: NSMenu) {
        if menu === tokenSpeedMenu {
            refreshTelemetry()
        }
    }

    @objc private func selectLanguage(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String,
              let option = Language(rawValue: raw) else { return }
        UserDefaults.standard.set(option.rawValue, forKey: languageDefaultsKey)
        rebuildMenu()
    }

    /// Quitting ModelMuxBar is quitting the whole stack: stop the CPA
    /// service, stop the ModelMux proxy (which also restores the managed
    /// Codex configuration), then terminate.
    @objc private func confirmQuit(_ sender: NSMenuItem) {
        let l10n = self.l10n
        let alert = NSAlert()
        alert.messageText = l10n.quitDialogTitle
        alert.informativeText = l10n.quitDialogBody
        alert.addButton(withTitle: l10n.quitDialogConfirm)
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            // ModelMux uninstall restores the Codex configuration and stops
            // its LaunchAgent; CPA stop shuts down the local proxy.
            _ = self?.captureModelmux(["uninstall"])
            _ = self?.captureModelmux(["cpa", "stop"])
            DispatchQueue.main.async {
                NSApp.terminate(nil)
            }
        }
    }

    @objc private func restartProxy() {
        // Restarting ModelMux means reinstalling its LaunchAgent, which also
        // re-enables the managed Codex configuration.
        runModelmuxDetached(["install"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.restartFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func stopProxy() {
        runModelmuxDetached(["uninstall"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.stopFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func startCPA() {
        runModelmuxDetached(["cpa", "start"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.startCPAFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func stopCPA() {
        runModelmuxDetached(["cpa", "stop"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.stopCPAFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func selectReviewModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        let arg = slug.isEmpty ? "" : slug
        runModelmuxDetached(["cpa", "review-set", arg]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.reviewSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func switchProfile(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        runModelmuxDetached(["cpa", "profile-switch", name]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.profileSwitchFailed(name) ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func openLogs() {
        NSWorkspace.shared.open(URL(fileURLWithPath: modelmuxHome + "/logs"))
    }

    // MARK: - Process helpers

    private func runModelmux(_ arguments: [String], completion: @escaping (String) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            completion(self.captureModelmux(arguments))
        }
    }

    /// Run modelmux synchronously and return its combined output (background queue only).
    private func captureModelmux(_ arguments: [String]) -> String {
        let process = Process()
        process.executableURL = modelmuxURL
        process.arguments = arguments
        process.environment = [
            "MODELMUX_HOME": modelmuxHome,
            "CODEX_CONFIG": codexConfig,
            "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
        ]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        do {
            try process.run()
        } catch {
            return ""
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return String(data: data, encoding: .utf8) ?? ""
    }

    private func runModelmuxDetached(_ arguments: [String], completion: @escaping (Bool) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let process = Process()
            process.executableURL = self.modelmuxURL
            process.arguments = arguments
            process.environment = [
                "MODELMUX_HOME": self.modelmuxHome,
                "CODEX_CONFIG": self.codexConfig,
                "PATH": "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
            ]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = pipe
            do {
                try process.run()
            } catch {
                DispatchQueue.main.async { completion(false) }
                return
            }
            _ = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            let ok = process.terminationStatus == 0
            DispatchQueue.main.async {
                completion(ok)
                self.refreshStatus()
            }
        }
    }

    private func showAlert(_ message: String) {
        DispatchQueue.main.async {
            let alert = NSAlert()
            alert.messageText = message
            alert.addButton(withTitle: self.l10n.alertOK)
            alert.runModal()
        }
    }
}

/// Bootstrap NSApplication, install the delegate, and run the event loop so
/// the status item actually renders.
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
