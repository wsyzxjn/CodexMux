import Cocoa
import Combine

/// User-facing strings for every language CodexMux supports.
struct L10n {
    let codexmuxStatusRunning: String
    let codexmuxStatusStopped: String
    let cpaStatusRunning: String
    let cpaStatusStopped: String
    let cpaStatusNotInstalled: String
    let controlsRunning: String
    let controlsStopped: String
    let restartCodexMux: String
    let stopCodexMux: String
    let startCPA: String
    let stopCPA: String
    let installCPA: String
    let cpaAutostart: String
    let openLogs: String
    let openCpaManagement: String
    let copyCpaManagementKey: String
    let profiles: String
    let profileNoProfiles: String
    let directEndpoints: String
    let directNoEndpoints: String
    let directAdd: String
    let directRemove: String
    let reviewModel: String
    let reviewDefault: String
    let profileActiveSuffix: String
    let language: String
    let quit: String
    let restartFailed: String
    let stopFailed: String
    let startCPAFailed: String
    let stopCPAFailed: String
    let installCPAFailed: String
    let autostartSetFailed: String
    let openCpaManagementFailed: String
    let copyCpaManagementKeyFailed: String
    let reviewSetFailed: String
    let directSetFailed: String
    let quitDialogTitle: String
    let quitDialogBody: String
    let quitDialogConfirm: String
    let quitDialogCancel: String
    let alertOK: String
    let directDialogTitle: String
    let directDialogBaseURL: String
    let directDialogToken: String
    let directDialogModels: String
    let directDialogHint: String

    static let english = L10n(
        codexmuxStatusRunning: "CodexMux: running",
        codexmuxStatusStopped: "CodexMux: stopped",
        cpaStatusRunning: "CPA service: running",
        cpaStatusStopped: "CPA service: stopped",
        cpaStatusNotInstalled: "CPA: not installed",
        controlsRunning: "Services ▸",
        controlsStopped: "Services… ▸",
        restartCodexMux: "Restart CodexMux",
        stopCodexMux: "Stop CodexMux",
        startCPA: "Start CPA service",
        stopCPA: "Stop CPA service",
        installCPA: "Install CPA…",
        cpaAutostart: "Start CPA with CodexMux",
        openLogs: "Open Logs Folder",
        openCpaManagement: "Open CPA Web Management",
        copyCpaManagementKey: "Copy CPA Management Key",
        profiles: "CPA Profiles",
        profileNoProfiles: "No saved profiles",
        directEndpoints: "Direct Endpoints",
        directNoEndpoints: "No direct endpoints",
        directAdd: "Add Direct Endpoint…",
        directRemove: "Remove",
        reviewModel: "Review Model",
        reviewDefault: "Default (official route)",
        profileActiveSuffix: "  ✓",
        language: "Language",
        quit: "Quit CodexMux",
        restartFailed: "Failed to restart CodexMux. See logs.",
        stopFailed: "Failed to stop CodexMux. See logs.",
        startCPAFailed: "Failed to start the CPA service. See logs.",
        stopCPAFailed: "Failed to stop the CPA service. See logs.",
        installCPAFailed: "Failed to install CPA. Check your network and see logs.",
        autostartSetFailed: "Failed to save the CPA startup preference. See logs.",
        openCpaManagementFailed: "Failed to open CPA Web Management. Check the CPA endpoint.",
        copyCpaManagementKeyFailed: "Failed to copy the CPA management key. Update CodexMux and try again.",
        reviewSetFailed: "Failed to set the review model. See logs.",
        directSetFailed: "Failed to save the direct endpoint. See logs.",
        quitDialogTitle: "Quit CodexMux?",
        quitDialogBody: "This stops the CodexMux proxy and the CPA service, and restores the Codex configuration.",
        quitDialogConfirm: "Quit and Stop Services",
        quitDialogCancel: "Cancel",
        alertOK: "OK",
        directDialogTitle: "Add Direct Endpoint",
        directDialogBaseURL: "Base URL:",
        directDialogToken: "Token:",
        directDialogModels: "Models (comma-separated):",
        directDialogHint: "Models route as cpa/<slug> straight to this endpoint; CPA is bypassed."
    )

    static let chinese = L10n(
        codexmuxStatusRunning: "CodexMux：运行中",
        codexmuxStatusStopped: "CodexMux：已停止",
        cpaStatusRunning: "CPA 服务：运行中",
        cpaStatusStopped: "CPA 服务：已停止",
        cpaStatusNotInstalled: "CPA：未安装",
        controlsRunning: "服务控制 ▸",
        controlsStopped: "服务控制… ▸",
        restartCodexMux: "重启 CodexMux",
        stopCodexMux: "停止 CodexMux",
        startCPA: "启动 CPA 服务",
        stopCPA: "停止 CPA 服务",
        installCPA: "安装 CPA…",
        cpaAutostart: "随 CodexMux 启动 CPA",
        openLogs: "打开日志文件夹",
        openCpaManagement: "打开 CPA Web 管理",
        copyCpaManagementKey: "复制 CPA 管理密钥",
        profiles: "CPA 配置",
        profileNoProfiles: "（暂无保存的配置）",
        directEndpoints: "直接端点",
        directNoEndpoints: "（暂无直接端点）",
        directAdd: "添加直接端点…",
        directRemove: "移除",
        reviewModel: "审批模型",
        reviewDefault: "默认（官方路由）",
        profileActiveSuffix: "  ✓",
        language: "语言",
        quit: "退出 CodexMux",
        restartFailed: "重启 CodexMux 失败，请查看日志。",
        stopFailed: "停止 CodexMux 失败，请查看日志。",
        startCPAFailed: "启动 CPA 服务失败，请查看日志。",
        stopCPAFailed: "停止 CPA 服务失败，请查看日志。",
        installCPAFailed: "安装 CPA 失败，请检查网络并查看日志。",
        autostartSetFailed: "保存 CPA 启动偏好失败，请查看日志。",
        openCpaManagementFailed: "打开 CPA Web 管理失败，请检查 CPA 地址。",
        copyCpaManagementKeyFailed: "复制 CPA 管理密钥失败，请更新 CodexMux 后重试。",
        reviewSetFailed: "设置审批模型失败，请查看日志。",
        directSetFailed: "保存直接端点失败，请查看日志。",
        quitDialogTitle: "退出 CodexMux？",
        quitDialogBody: "将停止 CodexMux 代理与 CPA 服务，并还原 Codex 配置。",
        quitDialogConfirm: "退出并停止服务",
        quitDialogCancel: "取消",
        alertOK: "好",
        directDialogTitle: "添加直接端点",
        directDialogBaseURL: "基础 URL：",
        directDialogToken: "令牌：",
        directDialogModels: "模型（逗号分隔）：",
        directDialogHint: "模型将以 cpa/<slug> 直接路由到该端点，绕过 CPA。"
    )

    /// True when this is the Chinese localization.
    var isChinese: Bool { quit == "退出 CodexMux" }

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

extension Array where Element: Hashable {
    /// Remove duplicate entries while keeping the original order.
    func unique() -> [Element] {
        var seen: Set<Element> = []
        return filter { seen.insert($0).inserted }
    }
}

/// Menu bar controller for CodexMux: service status, start/stop, and log access.
///
/// The menu bar app only manages CodexMux's own lifecycle (and its bundled CPA
/// service). General model selection stays in Codex; the only routing control
/// here is the explicit codex-auto-review override.
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var menu: NSMenu!
    private var timer: Timer?
    private var cancellables = Set<AnyCancellable>()

    private let codexmuxURL = URL(fileURLWithPath: NSString(
        string: "~/.local/bin/codexmux"
    ).expandingTildeInPath)
    private let codexmuxHome: String = {
        let environment = ProcessInfo.processInfo.environment
        if let configured = environment["CODEXMUX_HOME"], !configured.isEmpty {
            return NSString(string: configured).expandingTildeInPath
        }
        return NSString(
            string: "~/Library/Application Support/CodexMux"
        ).expandingTildeInPath
    }()
    private let languageDefaultsKey = "language"
    private var proxyReachable = false
    private var proxyIdle = false
    private var cpaRunning = false
    private var cpaInstalled = false
    private var cpaAutostart: Bool?
    private var activeProfile: String?
    private var savedProfiles: [(name: String, baseURL: String)] = []
    private var reviewOverride: String?
    private var cpaModels: [String] = []
    private var directRoutes: [(baseURL: String, models: [String])] = []

    private var language: Language {
        Language(rawValue: UserDefaults.standard.string(forKey: languageDefaultsKey) ?? "") ?? .systemPreferred
    }

    private var l10n: L10n { L10n.forLanguage(language) }

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        updateIcon()
        menu = NSMenu()
        menu.autoenablesItems = false
        statusItem.menu = menu
        rebuildMenu()
        refreshStatus()
        // The menu bar app is the controller: opening it brings the proxy up
        // and enables the managed Codex configuration from this GUI context
        // (LaunchAgent serve uses --no-codex-config because of TCC).
        runCodexMuxDetached(["install"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.restartFailed ?? "")
            }
            // launchd now wakes the proxy and an enabled local CPA only when
            // Codex Desktop or CLI actually connects.
            self?.refreshStatus()
        }
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            self?.refreshStatus()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        timer?.invalidate()
    }

    // MARK: - Status

    private func refreshStatus() {
        let group = DispatchGroup()
        group.enter()
        checkProxy { [weak self] available, idle in
            self?.proxyReachable = available
            self?.proxyIdle = idle
            group.leave()
        }
        group.enter()
        checkCPA { [weak self] running, installed, autostart in
            self?.cpaRunning = running
            self?.cpaInstalled = installed
            self?.cpaAutostart = autostart
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
        group.enter()
        loadDirectRoutes { [weak self] routes in
            self?.directRoutes = routes
            group.leave()
        }
        group.notify(queue: .main) { [weak self] in
            self?.updateIcon()
            self?.rebuildMenu()
        }
    }

    /// Load the review override and the CPA model list (background queue only).
    private func loadReviewState(
        _ completion: @escaping (String?, [String]) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let overrideOutput = self.captureCodexMux(["cpa", "review-get"])
            let overrideSlug: String? = {
                let line = overrideOutput.split(separator: "\n").first { $0.hasPrefix("review override: ") }
                guard let line else { return nil }
                let value = line.dropFirst("review override: ".count)
                return value.hasPrefix("(none") ? nil : String(value)
            }()
            let models = self.captureCodexMux(["cpa", "model-list"])
                .split(whereSeparator: \.isNewline)
                .map(String.init)
            DispatchQueue.main.async { completion(overrideSlug, models) }
        }
    }

    /// Parse `codexmux cpa profile-list` output (background queue only).
    private func loadProfiles(_ completion: @escaping (String?, [(name: String, baseURL: String)]) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["cpa", "profile-list"])
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

    private func checkProxy(_ completion: @escaping (Bool, Bool) -> Void) {
        runCodexMux(["status"]) { output in
            let running = output.contains("proxy service: running")
            let idle = output.contains("proxy service: idle")
            completion(running || idle, idle)
        }
    }

    private func checkCPA(_ completion: @escaping (Bool, Bool, Bool?) -> Void) {
        runCodexMux(["cpa", "status"]) { output in
            let running = output.contains("service: running")
            let installed = output.contains("binary: installed")
            var autostart: Bool?
            if output.contains("autostart: enabled") {
                autostart = true
            } else if output.contains("autostart: disabled") {
                autostart = false
            }
            completion(running, installed, autostart)
        }
    }

    /// Parse `codexmux cpa direct-list` output into per-endpoint routes.
    private func loadDirectRoutes(_ completion: @escaping ([(baseURL: String, models: [String])]) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["cpa", "direct-list"])
            var routes: [(baseURL: String, models: [String])] = []
            for rawLine in output.split(whereSeparator: \.isNewline) {
                // "model-a, model-b -> https://example.com/v1"
                guard let arrow = rawLine.range(of: " -> ") else { continue }
                let modelsText = rawLine[..<arrow.lowerBound]
                let baseURLText = rawLine[arrow.upperBound...]
                var models: [String] = []
                for rawModel in modelsText.split(separator: ",") {
                    let model = rawModel.trimmingCharacters(in: .whitespaces)
                    if !model.isEmpty {
                        models.append(model)
                    }
                }
                let baseURL = baseURLText.trimmingCharacters(in: .whitespaces)
                if !models.isEmpty && !baseURL.isEmpty {
                    routes.append((baseURL: baseURL, models: models))
                }
            }
            DispatchQueue.main.async { completion(routes) }
        }
    }

    private func updateIcon() {
        guard let button = statusItem.button else { return }
        let label = proxyReachable ? (proxyIdle ? "CodexMux idle" : "CodexMux") : "CodexMux stopped"
        button.image = nil
        button.title = ">_<"
        button.font = .monospacedSystemFont(ofSize: 13, weight: .semibold)
        button.toolTip = label
        button.setAccessibilityLabel(label)
    }

    // MARK: - Menu

    private func rebuildMenu() {
        let l10n = self.l10n
        menu.removeAllItems()

        // Status lines live in collapsible submenus so the top level stays short.
        let proxyStatusTitle = proxyIdle
            ? (l10n.isChinese ? "CodexMux：待机" : "CodexMux: idle")
            : (proxyReachable ? l10n.codexmuxStatusRunning : l10n.codexmuxStatusStopped)
        let codexmuxStatusItem = NSMenuItem(
            title: proxyStatusTitle,
            action: nil, keyEquivalent: ""
        )
        codexmuxStatusItem.isEnabled = false
        menu.addItem(codexmuxStatusItem)

        let cpaStatusItem = NSMenuItem(
            title: cpaInstalled
                ? (cpaRunning ? l10n.cpaStatusRunning : l10n.cpaStatusStopped)
                : l10n.cpaStatusNotInstalled,
            action: nil, keyEquivalent: ""
        )
        cpaStatusItem.isEnabled = false
        menu.addItem(cpaStatusItem)

        // Controls submenu: start/stop actions for both services.
        let controlsTitle = cpaRunning
            ? l10n.controlsRunning
            : l10n.controlsStopped
        let controlsItem = NSMenuItem(title: controlsTitle, action: nil, keyEquivalent: "")
        let controls = NSMenu()
        controls.autoenablesItems = false

        let restart = NSMenuItem(title: l10n.restartCodexMux, action: #selector(restartProxy),
                                 keyEquivalent: "r")
        restart.target = self
        restart.isEnabled = true
        controls.addItem(restart)

        let stop = NSMenuItem(title: l10n.stopCodexMux, action: #selector(stopProxy),
                              keyEquivalent: "s")
        stop.target = self
        stop.isEnabled = proxyReachable
        controls.addItem(stop)

        controls.addItem(.separator())

        if cpaInstalled {
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
        } else {
            let installItem = NSMenuItem(title: l10n.installCPA, action: #selector(installCPA),
                                         keyEquivalent: "")
            installItem.target = self
            installItem.isEnabled = true
            controls.addItem(installItem)
        }

        // Startup preference: whether CPA starts together with CodexMux.
        let autostartItem = NSMenuItem(title: l10n.cpaAutostart,
                                       action: #selector(toggleCPAAutostart),
                                       keyEquivalent: "")
        autostartItem.target = self
        autostartItem.isEnabled = cpaInstalled
        autostartItem.state = (cpaAutostart == true) ? .on : .off
        controls.addItem(autostartItem)
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

        // Direct endpoints submenu: upstreams CodexMux routes to directly,
        // bypassing CPA. Works with or without CPA installed.
        let directItem = NSMenuItem(title: l10n.directEndpoints, action: nil, keyEquivalent: "")
        let directMenu = NSMenu()
        directMenu.autoenablesItems = false
        if directRoutes.isEmpty {
            let empty = NSMenuItem(title: l10n.directNoEndpoints, action: nil, keyEquivalent: "")
            empty.isEnabled = false
            directMenu.addItem(empty)
        } else {
            for route in directRoutes {
                let host = URL(string: route.baseURL)?.host ?? route.baseURL
                let title = "\(host) (\(route.models.count))"
                let item = NSMenuItem(title: title,
                                      action: #selector(removeDirectRoute(_:)),
                                      keyEquivalent: "")
                item.target = self
                item.representedObject = route.baseURL
                item.toolTip = "\(route.models.joined(separator: ", ")) → \(route.baseURL)"
                directMenu.addItem(item)
            }
        }
        let directAddItem = NSMenuItem(title: l10n.directAdd,
                                       action: #selector(addDirectRoute),
                                       keyEquivalent: "")
        directAddItem.target = self
        directMenu.addItem(.separator())
        directMenu.addItem(directAddItem)
        directItem.submenu = directMenu
        menu.addItem(directItem)

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

        for slug in (cpaModels + directRoutes.flatMap(\.models)).sorted().unique() {
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

        let cpaManagement = NSMenuItem(title: l10n.openCpaManagement,
                                       action: #selector(openCpaManagement),
                                       keyEquivalent: "")
        cpaManagement.target = self
        menu.addItem(cpaManagement)

        let copyManagementKey = NSMenuItem(title: l10n.copyCpaManagementKey,
                                           action: #selector(copyCpaManagementKey),
                                           keyEquivalent: "")
        copyManagementKey.target = self
        menu.addItem(copyManagementKey)

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

    @objc private func selectLanguage(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String,
              let option = Language(rawValue: raw) else { return }
        UserDefaults.standard.set(option.rawValue, forKey: languageDefaultsKey)
        rebuildMenu()
    }

    /// Quitting the menu bar app is quitting the whole stack: stop the CPA
    /// service, stop the CodexMux proxy (which also restores the managed
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
            // CodexMux uninstall restores the Codex configuration and stops
            // its LaunchAgent; CPA stop shuts down the local proxy without
            // touching the startup preference, so the next launch honors the
            // user's last explicit choice.
            _ = self?.captureCodexMux(["uninstall"])
            _ = self?.captureCodexMux(["cpa", "stop", "--no-preference"])
            DispatchQueue.main.async {
                NSApp.terminate(nil)
            }
        }
    }

    @objc private func restartProxy() {
        // Restarting CodexMux means reinstalling its LaunchAgent, which also
        // re-enables the managed Codex configuration.
        runCodexMuxDetached(["install"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.restartFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func stopProxy() {
        runCodexMuxDetached(["uninstall"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.stopFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func startCPA() {
        runCodexMuxDetached(["cpa", "start"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.startCPAFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func stopCPA() {
        runCodexMuxDetached(["cpa", "stop"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.stopCPAFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func installCPA() {
        runCodexMuxDetached(["cpa", "install"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.installCPAFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func toggleCPAAutostart(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["cpa", "autostart-set", enabled ? "true" : "false"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.autostartSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func addDirectRoute() {
        let l10n = self.l10n
        let alert = NSAlert()
        alert.messageText = l10n.directDialogTitle
        alert.informativeText = l10n.directDialogHint

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6

        let baseURLField = NSTextField()
        baseURLField.placeholderString = "https://example.com/v1"
        baseURLField.widthAnchor.constraint(equalToConstant: 320).isActive = true
        let tokenField = NSSecureTextField()
        let modelsField = NSTextField()
        modelsField.placeholderString = "gpt-5.6-sol, gpt-5.6-terra"

        func row(_ label: String, _ field: NSView) -> NSView {
            let container = NSStackView()
            container.orientation = .horizontal
            container.spacing = 8
            let text = NSTextField(labelWithString: label)
            text.widthAnchor.constraint(equalToConstant: 150).isActive = true
            container.addArrangedSubview(text)
            container.addArrangedSubview(field)
            return container
        }
        stack.addArrangedSubview(row(l10n.directDialogBaseURL, baseURLField))
        stack.addArrangedSubview(row(l10n.directDialogToken, tokenField))
        stack.addArrangedSubview(row(l10n.directDialogModels, modelsField))
        alert.accessoryView = stack
        alert.addButton(withTitle: l10n.alertOK)
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        let baseURL = baseURLField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let token = tokenField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        var models: [String] = []
        for rawModel in modelsField.stringValue.split(separator: ",") {
            let model = rawModel.trimmingCharacters(in: .whitespacesAndNewlines)
            if !model.isEmpty {
                models.append(model)
            }
        }
        guard !baseURL.isEmpty, !token.isEmpty, !models.isEmpty else { return }

        // The CLI reads the upstream token from the environment so it never
        // appears in process arguments or menu logs.
        var environment = codexMuxEnvironment
        environment["CODEXMUX_DIRECT_TOKEN"] = token
        runCodexMuxDetached(
            ["cpa", "direct-add", models.joined(separator: ","), "--base-url", baseURL],
            environment: environment
        ) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.directSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func removeDirectRoute(_ sender: NSMenuItem) {
        guard let baseURL = sender.representedObject as? String else { return }
        runCodexMuxDetached(
            ["cpa", "direct-remove", "--base-url", baseURL]
        ) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.directSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func selectReviewModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "review-set", slug]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.reviewSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func switchProfile(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "profile-switch", name]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.profileSwitchFailed(name) ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func openLogs() {
        NSWorkspace.shared.open(URL(fileURLWithPath: codexmuxHome + "/logs"))
    }

    @objc private func openCpaManagement() {
        runCodexMux(["cpa", "management-url", "--connect"]) { [weak self] output in
            let url = output
                .split(whereSeparator: \.isNewline)
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                .compactMap { URL(string: $0) }
                .first { $0.scheme == "http" || $0.scheme == "https" }
            DispatchQueue.main.async {
                guard let url, NSWorkspace.shared.open(url) else {
                    self?.showAlert(self?.l10n.openCpaManagementFailed ?? "")
                    return
                }
            }
        }
    }

    @objc private func copyCpaManagementKey() {
        runCodexMux(["cpa", "management-key"]) { [weak self] output in
            let prefix = "management-key: "
            let key = output
                .split(whereSeparator: \.isNewline)
                .map(String.init)
                .first { $0.hasPrefix(prefix) }
                .map { String($0.dropFirst(prefix.count)) }
            DispatchQueue.main.async {
                guard let key, !key.isEmpty else {
                    self?.showAlert(self?.l10n.copyCpaManagementKeyFailed ?? "")
                    return
                }
                NSPasteboard.general.clearContents()
                guard NSPasteboard.general.setString(key, forType: .string) else {
                    self?.showAlert(self?.l10n.copyCpaManagementKeyFailed ?? "")
                    return
                }
            }
        }
    }

    // MARK: - Process helpers

    private func runCodexMux(_ arguments: [String], completion: @escaping (String) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            completion(self.captureCodexMux(arguments))
        }
    }

    private var codexMuxEnvironment: [String: String] {
        let inherited = ProcessInfo.processInfo.environment
        var environment = [
            "CODEXMUX_HOME": codexmuxHome,
            "PATH": "\(NSHomeDirectory())/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
        ]
        for name in ["CODEX_CONFIG", "CODEX_HOME", "HOME", "TMPDIR", "LANG", "LC_ALL"] {
            if let value = inherited[name], !value.isEmpty {
                environment[name] = value
            }
        }
        return environment
    }

    /// Run codexmux synchronously and return its combined output (background queue only).
    private func captureCodexMux(_ arguments: [String]) -> String {
        let process = Process()
        process.executableURL = codexmuxURL
        process.arguments = arguments
        process.environment = codexMuxEnvironment
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        do {
            try process.run()
        } catch {
            return "codexmux is not installed at \(codexmuxURL.path)"
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return String(data: data, encoding: .utf8) ?? ""
    }

    private func runCodexMuxDetached(
        _ arguments: [String],
        environment: [String: String]? = nil,
        completion: @escaping (Bool) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let process = Process()
            process.executableURL = self.codexmuxURL
            process.arguments = arguments
            process.environment = environment ?? self.codexMuxEnvironment
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

/// Entry point: bootstrap NSApplication, install the delegate, and run the
/// event loop so the status item actually renders.
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
