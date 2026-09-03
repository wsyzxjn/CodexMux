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
    let installingCPA: String
    let cpaAutostart: String
    let advertiseUltra: String
    let advertiseUltraFailed: String
    let cpaUpdate: String
    let cpaVersionUnknown: String
    let cpaCheckUpdate: String
    let cpaRollback: String
    let cpaUpdateCheckFailed: String
    let cpaUpdateFailed: String
    let cpaRollbackFailed: String
    let cpaUpdateDialogTitle: String
    let cpaRollbackDialogTitle: String
    let cpaRollbackDialogBody: String
    let openLogs: String
    let openCpaManagement: String
    let copyCpaManagementKey: String
    let profiles: String
    let profileNoProfiles: String
    let directEndpoints: String
    let directNoEndpoints: String
    let directAdd: String
    let directRemove: String
    let advanced: String
    let reviewModel: String
    let reviewDefault: String
    let profileActiveSuffix: String
    let language: String
    let about: String
    let checkAppUpdates: String
    let checkingAppUpdates: String
    let openRepository: String
    let appUpToDate: String
    let appUpdateCheckFailed: String
    let appUpdateFailed: String
    let appUpdateDialogTitle: String
    let appUpdateDialogInstall: String
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
        installCPA: "Download and Enable CPA…",
        installingCPA: "Downloading and enabling CPA…",
        cpaAutostart: "Start CPA with CodexMux",
        advertiseUltra: "Advertise Ultra for All Models",
        advertiseUltraFailed: "Failed to save the Ultra catalog setting. See logs.",
        cpaUpdate: "CPA Update",
        cpaVersionUnknown: "Version: unknown",
        cpaCheckUpdate: "Check for CPA Updates…",
        cpaRollback: "Roll Back CPA…",
        cpaUpdateCheckFailed: "Failed to check for CPA updates. See logs.",
        cpaUpdateFailed: "Failed to update CPA. The previous version was restored if possible. See logs.",
        cpaRollbackFailed: "Failed to roll back CPA. The current version was kept. See logs.",
        cpaUpdateDialogTitle: "Update CPA?",
        cpaRollbackDialogTitle: "Roll Back CPA?",
        cpaRollbackDialogBody: "Restore the previous CLIProxyAPI version and restart the service. The current version is kept if validation fails.",
        openLogs: "Open Logs Folder",
        openCpaManagement: "Open CPA Web Management",
        copyCpaManagementKey: "Copy CPA Management Key",
        profiles: "CPA Profiles",
        profileNoProfiles: "No saved profiles",
        directEndpoints: "Direct Endpoints",
        directNoEndpoints: "No direct endpoints",
        directAdd: "Add Direct Endpoint…",
        directRemove: "Remove",
        advanced: "Advanced",
        reviewModel: "Review Model",
        reviewDefault: "Default (official route)",
        profileActiveSuffix: "  ✓",
        language: "Language",
        about: "About CodexMux…",
        checkAppUpdates: "Check for CodexMux Updates…",
        checkingAppUpdates: "Checking for updates…",
        openRepository: "Open Repository",
        appUpToDate: "CodexMux is up to date.",
        appUpdateCheckFailed: "Failed to check for CodexMux updates.",
        appUpdateFailed: "Failed to prepare the CodexMux update. The installed app was not changed.",
        appUpdateDialogTitle: "Update CodexMux?",
        appUpdateDialogInstall: "Install and Restart",
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
        installCPA: "下载并启用 CPA…",
        installingCPA: "正在下载并启用 CPA…",
        cpaAutostart: "随 CodexMux 启动 CPA",
        advertiseUltra: "为所有模型声明 Ultra",
        advertiseUltraFailed: "保存 Ultra 目录设置失败，请查看日志。",
        cpaUpdate: "CPA 更新",
        cpaVersionUnknown: "版本：未知",
        cpaCheckUpdate: "检查 CPA 更新…",
        cpaRollback: "回滚 CPA…",
        cpaUpdateCheckFailed: "检查 CPA 更新失败，请查看日志。",
        cpaUpdateFailed: "更新 CPA 失败，已尽量恢复上一版本，请查看日志。",
        cpaRollbackFailed: "回滚 CPA 失败，已保留当前版本，请查看日志。",
        cpaUpdateDialogTitle: "更新 CPA？",
        cpaRollbackDialogTitle: "回滚 CPA？",
        cpaRollbackDialogBody: "将恢复上一版 CLIProxyAPI 并重启服务；校验失败时会保留当前版本。",
        openLogs: "打开日志文件夹",
        openCpaManagement: "打开 CPA Web 管理",
        copyCpaManagementKey: "复制 CPA 管理密钥",
        profiles: "CPA 配置",
        profileNoProfiles: "（暂无保存的配置）",
        directEndpoints: "直接端点",
        directNoEndpoints: "（暂无直接端点）",
        directAdd: "添加直接端点…",
        directRemove: "移除",
        advanced: "高级功能",
        reviewModel: "审批模型",
        reviewDefault: "默认（官方路由）",
        profileActiveSuffix: "  ✓",
        language: "语言",
        about: "关于 CodexMux…",
        checkAppUpdates: "检查 CodexMux 更新…",
        checkingAppUpdates: "正在检查更新…",
        openRepository: "打开仓库",
        appUpToDate: "CodexMux 已是最新版本。",
        appUpdateCheckFailed: "检查 CodexMux 更新失败。",
        appUpdateFailed: "准备 CodexMux 更新失败，已安装的 App 未被修改。",
        appUpdateDialogTitle: "更新 CodexMux？",
        appUpdateDialogInstall: "安装并重新启动",
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

    func cpaUpdateAvailable(_ version: String) -> String {
        isChinese ? "可用更新：\(version)" : "Update available: \(version)"
    }

    func cpaUpToDate(_ version: String) -> String {
        isChinese ? "已是最新：\(version)" : "Up to date: \(version)"
    }

    func cpaUpdateTo(_ version: String) -> String {
        isChinese ? "更新到 \(version)…" : "Update to \(version)…"
    }

    func cpaUpdateDialogBody(from: String, to: String) -> String {
        if isChinese {
            return "将受管的本地 CPA 从 \(from) 更新到 \(to)。更新后会重启并校验服务；失败时自动恢复上一版本。"
        }
        return "Update the managed local CPA from \(from) to \(to). The service is restarted and validated; the previous version is restored on failure."
    }

    func appUpdateDialogBody(from: String, to: String) -> String {
        if isChinese {
            return "将 CodexMux 从 \(from) 更新到 \(to)。下载内容会经过校验，当前 App 会在替换前保留备份。"
        }
        return "Update CodexMux from \(from) to \(to). The download is verified and the current app is backed up before replacement."
    }

    func appUpdateAvailable(_ version: String) -> String {
        isChinese ? "可用更新：\(version)" : "Update available: \(version)"
    }

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
    private static let repositoryURL = URL(string: "https://github.com/wsyzxjn/CodexMux")!
    private var statusItem: NSStatusItem!
    private var menu: NSMenu!
    private var timer: Timer?
    private var cancellables = Set<AnyCancellable>()
    private let appUpdater = AppUpdater(repository: "wsyzxjn/CodexMux")
    private var aboutWindowController: AboutWindowController?
    private var appUpdateInProgress = false

    /// Release builds are self-contained. Copy the bundled CLI to a stable
    /// private runtime path so LaunchAgents keep working if the App is moved.
    private lazy var codexmuxURL: URL = {
        if let bundled = Bundle.main.url(forResource: "codexmux", withExtension: nil) {
            let runtimeDirectory = URL(fileURLWithPath: codexmuxHome, isDirectory: true)
                .appendingPathComponent("bin", isDirectory: true)
            let runtime = runtimeDirectory.appendingPathComponent("codexmux")
            do {
                try FileManager.default.createDirectory(
                    at: runtimeDirectory,
                    withIntermediateDirectories: true
                )
                let bundledData = try Data(contentsOf: bundled)
                let installedData = try? Data(contentsOf: runtime)
                if installedData != bundledData {
                    try bundledData.write(to: runtime, options: .atomic)
                }
                try FileManager.default.setAttributes(
                    [.posixPermissions: 0o755],
                    ofItemAtPath: runtime.path
                )
                return runtime
            } catch {
                return bundled
            }
        }
        return URL(fileURLWithPath: NSString(
            string: "~/.local/bin/codexmux"
        ).expandingTildeInPath)
    }()
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
    private var cpaInstalling = false
    private var cpaAutostart: Bool?
    private var advertiseUltra = false
    private var cpaInstalledVersion: String?
    private var cpaLatestVersion: String?
    private var cpaUpdateAvailable = false
    private var cpaRollbackAvailable = false
    private var cpaUpdating = false
    private var cpaCheckingUpdate = false
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
            } else {
                self?.relaunchRunningCodex()
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
        checkCPA { [weak self] running, installed, autostart, version, rollback in
            self?.cpaRunning = running
            self?.cpaInstalled = installed
            self?.cpaAutostart = autostart
            self?.cpaInstalledVersion = version
            self?.cpaRollbackAvailable = rollback
            group.leave()
        }
        group.enter()
        loadUltraState { [weak self] enabled in
            self?.advertiseUltra = enabled
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

    /// `codexmux install` places the private proxy token in the current GUI
    /// launchd environment. A running Codex process cannot observe that new
    /// value, so restart only an already-running Desktop app. If Codex was not
    /// open, its next launch naturally inherits the prepared environment.
    private func relaunchRunningCodex() {
        let applications = NSRunningApplication.runningApplications(
            withBundleIdentifier: "com.openai.codex"
        )
        guard let application = applications.first,
              let bundleURL = application.bundleURL else { return }

        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = true
        if !application.terminate() {
            return
        }

        DispatchQueue.global(qos: .userInitiated).async {
            for _ in 0..<50 {
                if application.isTerminated { break }
                Thread.sleep(forTimeInterval: 0.1)
            }
            guard application.isTerminated else { return }
            DispatchQueue.main.async {
                NSWorkspace.shared.openApplication(
                    at: bundleURL,
                    configuration: configuration
                )
            }
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

    /// Load whether CodexMux advertises `ultra` for every merged model.
    private func loadUltraState(_ completion: @escaping (Bool) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["catalog", "ultra-get"])
            let enabled = output
                .split(separator: "\n")
                .contains("ultra: true")
            DispatchQueue.main.async { completion(enabled) }
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

    private func checkCPA(_ completion: @escaping (Bool, Bool, Bool?, String?, Bool) -> Void) {
        runCodexMux(["cpa", "status"]) { output in
            let running = output.contains("service: running")
            let installed = output.contains("binary: installed")
            let installedVersion: String? = {
                let line = output
                    .split(separator: "\n")
                    .first { $0.hasPrefix("version: ") }
                return line.map { String($0.dropFirst("version: ".count)) }
            }()
            var autostart: Bool?
            if output.contains("autostart: enabled") {
                autostart = true
            } else if output.contains("autostart: disabled") {
                autostart = false
            }
            let rollback = output.contains("rollback: available")
            completion(running, installed, autostart, installedVersion, rollback)
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
        controlsItem.submenu = controls
        menu.addItem(controlsItem)

        // One CPA menu owns installation, lifecycle, updates, profiles,
        // review routing, and web management.
        let cpaItem = NSMenuItem(title: "CPA", action: nil, keyEquivalent: "")
        let cpaMenu = NSMenu()
        cpaMenu.autoenablesItems = false

        let cpaStatusItem = NSMenuItem(
            title: cpaInstalled
                ? (cpaRunning ? l10n.cpaStatusRunning : l10n.cpaStatusStopped)
                : l10n.cpaStatusNotInstalled,
            action: nil, keyEquivalent: ""
        )
        cpaStatusItem.isEnabled = false
        cpaMenu.addItem(cpaStatusItem)
        cpaMenu.addItem(.separator())

        if cpaInstalled {
            let startCPAItem = NSMenuItem(title: l10n.startCPA, action: #selector(startCPA),
                                          keyEquivalent: "")
            startCPAItem.target = self
            startCPAItem.isEnabled = !cpaRunning
            cpaMenu.addItem(startCPAItem)

            let stopCPAItem = NSMenuItem(title: l10n.stopCPA, action: #selector(stopCPA),
                                         keyEquivalent: "")
            stopCPAItem.target = self
            stopCPAItem.isEnabled = cpaRunning
            cpaMenu.addItem(stopCPAItem)
        } else {
            let installItem = NSMenuItem(title: cpaInstalling ? l10n.installingCPA : l10n.installCPA,
                                         action: #selector(installCPA),
                                         keyEquivalent: "")
            installItem.target = self
            installItem.isEnabled = !cpaInstalling
            cpaMenu.addItem(installItem)
        }

        let autostartItem = NSMenuItem(title: l10n.cpaAutostart,
                                       action: #selector(toggleCPAAutostart),
                                       keyEquivalent: "")
        autostartItem.target = self
        autostartItem.isEnabled = cpaInstalled
        autostartItem.state = (cpaAutostart == true) ? .on : .off
        cpaMenu.addItem(autostartItem)
        cpaMenu.addItem(.separator())

        // CPA update submenu: check, apply, and roll back the managed local
        // CLIProxyAPI release from the menu bar.
        let updateItem = NSMenuItem(title: l10n.cpaUpdate, action: nil, keyEquivalent: "")
        let updateMenu = NSMenu()
        updateMenu.autoenablesItems = false
        let updateStatus: String
        if let latest = cpaLatestVersion {
            updateStatus = cpaUpdateAvailable
                ? l10n.cpaUpdateAvailable(latest)
                : l10n.cpaUpToDate(cpaInstalledVersion ?? latest)
        } else {
            updateStatus = l10n.cpaVersionUnknown
        }
        let updateStatusItem = NSMenuItem(title: updateStatus, action: nil, keyEquivalent: "")
        updateStatusItem.isEnabled = false
        updateMenu.addItem(updateStatusItem)
        updateMenu.addItem(.separator())

        let checkUpdateItem = NSMenuItem(title: l10n.cpaCheckUpdate,
                                         action: #selector(checkCpaUpdate),
                                         keyEquivalent: "")
        checkUpdateItem.target = self
        checkUpdateItem.isEnabled = cpaInstalled && !cpaUpdating && !cpaCheckingUpdate
        updateMenu.addItem(checkUpdateItem)

        if let latest = cpaLatestVersion, cpaUpdateAvailable {
            let updateToItem = NSMenuItem(title: l10n.cpaUpdateTo(latest),
                                          action: #selector(updateCpa),
                                          keyEquivalent: "")
            updateToItem.target = self
            updateToItem.isEnabled = cpaInstalled && !cpaUpdating
            updateMenu.addItem(updateToItem)
        }
        if cpaRollbackAvailable {
            let rollbackItem = NSMenuItem(title: l10n.cpaRollback,
                                          action: #selector(rollbackCpa),
                                          keyEquivalent: "")
            rollbackItem.target = self
            rollbackItem.isEnabled = cpaInstalled && !cpaUpdating
            updateMenu.addItem(rollbackItem)
        }
        updateItem.submenu = updateMenu
        cpaMenu.addItem(updateItem)

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
        cpaMenu.addItem(profilesItem)

        cpaMenu.addItem(.separator())
        let cpaManagement = NSMenuItem(title: l10n.openCpaManagement,
                                       action: #selector(openCpaManagement),
                                       keyEquivalent: "")
        cpaManagement.target = self
        cpaManagement.isEnabled = cpaInstalled
        cpaMenu.addItem(cpaManagement)

        let copyManagementKey = NSMenuItem(title: l10n.copyCpaManagementKey,
                                           action: #selector(copyCpaManagementKey),
                                           keyEquivalent: "")
        copyManagementKey.target = self
        copyManagementKey.isEnabled = cpaInstalled
        cpaMenu.addItem(copyManagementKey)

        cpaItem.submenu = cpaMenu
        menu.addItem(cpaItem)

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
                let url = URL(string: route.baseURL)
                let host = url?.host ?? route.baseURL
                let path = (url?.path ?? "").trimmingCharacters(in: CharacterSet(charactersIn: "/"))
                let pathLabel = path.hasSuffix("/v1") ? String(path.dropLast(3)) : path
                let endpoint = pathLabel.isEmpty ? host : "\(host)/\(pathLabel)"
                let title = "\(endpoint) (\(route.models.count))"
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

        // Advanced catalog settings that affect how Codex sees merged models.
        let advancedItem = NSMenuItem(title: l10n.advanced, action: nil, keyEquivalent: "")
        let advancedMenu = NSMenu()
        advancedMenu.autoenablesItems = false
        let ultraItem = NSMenuItem(title: l10n.advertiseUltra,
                                   action: #selector(toggleAdvertiseUltra),
                                   keyEquivalent: "")
        ultraItem.target = self
        ultraItem.state = advertiseUltra ? .on : .off
        advancedMenu.addItem(ultraItem)
        advancedItem.submenu = advancedMenu
        menu.addItem(advancedItem)

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
        let about = NSMenuItem(title: l10n.about, action: #selector(showAbout),
                               keyEquivalent: "")
        about.target = self
        menu.addItem(about)

        let checkAppUpdates = NSMenuItem(
            title: appUpdateInProgress ? l10n.checkingAppUpdates : l10n.checkAppUpdates,
            action: #selector(checkAppUpdates),
            keyEquivalent: ""
        )
        checkAppUpdates.target = self
        checkAppUpdates.isEnabled = !appUpdateInProgress
        menu.addItem(checkAppUpdates)

        menu.addItem(.separator())
        let quit = NSMenuItem(title: l10n.quit, action: #selector(confirmQuit(_:)),
                              keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
    }

    // MARK: - Actions

    @objc private func showAbout() {
        let controller = aboutWindowController ?? AboutWindowController(
            repositoryURL: Self.repositoryURL
        )
        controller.configure(
            appVersion: appVersion,
            cliVersion: bundledCLIVersion,
            l10n: l10n,
            isChecking: appUpdateInProgress,
            onCheckForUpdates: { [weak self] in self?.beginAppUpdateCheck() }
        )
        aboutWindowController = controller
        controller.showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc private func checkAppUpdates() {
        beginAppUpdateCheck()
    }

    private var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
            ?? "unknown"
    }

    private var bundledCLIVersion: String {
        let output = captureCodexMux(["--version"])
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return output.isEmpty ? "unknown" : output
    }

    private func beginAppUpdateCheck() {
        guard !appUpdateInProgress else { return }
        appUpdateInProgress = true
        aboutWindowController?.setUpdateState(
            message: l10n.checkingAppUpdates,
            isChecking: true
        )
        rebuildMenu()

        appUpdater.checkForUpdate(currentVersion: appVersion) { [weak self] result in
            DispatchQueue.main.async {
                guard let self else { return }
                self.appUpdateInProgress = false
                self.rebuildMenu()
                switch result {
                case .success(nil):
                    self.aboutWindowController?.setUpdateState(
                        message: self.l10n.appUpToDate,
                        isChecking: false
                    )
                    if self.aboutWindowController?.window?.isVisible != true {
                        self.showAlert(self.l10n.appUpToDate)
                    }
                case .success(let release?):
                    self.confirmAppUpdate(release)
                case .failure:
                    self.aboutWindowController?.setUpdateState(
                        message: self.l10n.appUpdateCheckFailed,
                        isChecking: false
                    )
                    self.showAlert(self.l10n.appUpdateCheckFailed)
                }
            }
        }
    }

    private func confirmAppUpdate(_ release: AppRelease) {
        let alert = NSAlert()
        alert.messageText = l10n.appUpdateDialogTitle
        alert.informativeText = l10n.appUpdateDialogBody(
            from: appVersion,
            to: release.version
        )
        alert.addButton(withTitle: l10n.appUpdateDialogInstall)
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else {
            aboutWindowController?.setUpdateState(
                message: l10n.appUpdateAvailable(release.version),
                isChecking: false
            )
            return
        }

        appUpdateInProgress = true
        aboutWindowController?.setUpdateState(
            message: l10n.isChinese ? "正在下载并校验更新…" : "Downloading and verifying update…",
            isChecking: true
        )
        rebuildMenu()
        appUpdater.prepareUpdate(release) { [weak self] result in
            DispatchQueue.main.async {
                guard let self else { return }
                switch result {
                case .success(let prepared):
                    do {
                        try self.appUpdater.installAndRelaunch(prepared)
                    } catch {
                        self.appUpdateInProgress = false
                        self.rebuildMenu()
                        self.showAlert(self.l10n.appUpdateFailed)
                    }
                case .failure:
                    self.appUpdateInProgress = false
                    self.rebuildMenu()
                    self.aboutWindowController?.setUpdateState(
                        message: self.l10n.appUpdateFailed,
                        isChecking: false
                    )
                    self.showAlert(self.l10n.appUpdateFailed)
                }
            }
        }
    }

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
        guard !cpaInstalling else { return }
        cpaInstalling = true
        rebuildMenu()
        runCodexMuxDetached(["cpa", "install"]) { [weak self] ok in
            self?.cpaInstalling = false
            if !ok {
                self?.showAlert(self?.l10n.installCPAFailed ?? "")
            } else {
                // Installation also starts CPA and enables autostart. Hand
                // off directly to its Web UI for provider credentials.
                self?.openCpaManagement()
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

    @objc private func toggleAdvertiseUltra(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["catalog", "ultra-set", enabled ? "true" : "false"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.advertiseUltraFailed ?? "")
            } else {
                self?.restartProxy()
            }
        }
    }

    @objc private func checkCpaUpdate() {
        guard cpaInstalled, !cpaCheckingUpdate else { return }
        cpaCheckingUpdate = true
        rebuildMenu()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = self.captureCodexMuxResult(["cpa", "update-check"])
            let latest: String? = result.output
                .split(separator: "\n")
                .first { $0.hasPrefix("latest: ") }
                .map { String($0.dropFirst("latest: ".count)) }
            DispatchQueue.main.async {
                self.cpaCheckingUpdate = false
                guard result.status == 0, let latest, !latest.isEmpty else {
                    self.showAlert(self.l10n.cpaUpdateCheckFailed)
                    self.rebuildMenu()
                    return
                }
                self.cpaLatestVersion = latest
                self.cpaUpdateAvailable = result.output.contains("update available: true")
                self.rebuildMenu()
            }
        }
    }

    @objc private func updateCpa() {
        guard cpaInstalled, !cpaUpdating,
              let latest = cpaLatestVersion, cpaUpdateAvailable else { return }
        let alert = NSAlert()
        alert.messageText = l10n.cpaUpdateDialogTitle
        alert.informativeText = l10n.cpaUpdateDialogBody(
            from: cpaInstalledVersion ?? "unknown",
            to: latest
        )
        alert.addButton(withTitle: l10n.cpaUpdateTo(latest))
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        cpaUpdating = true
        rebuildMenu()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = self.captureCodexMuxResult(["cpa", "update"])
            DispatchQueue.main.async {
                self.cpaUpdating = false
                self.cpaLatestVersion = nil
                self.cpaUpdateAvailable = false
                if result.status != 0 {
                    self.showAlert(self.l10n.cpaUpdateFailed)
                }
                self.refreshStatus()
            }
        }
    }

    @objc private func rollbackCpa() {
        guard cpaInstalled, !cpaUpdating, cpaRollbackAvailable else { return }
        let alert = NSAlert()
        alert.messageText = l10n.cpaRollbackDialogTitle
        alert.informativeText = l10n.cpaRollbackDialogBody
        alert.addButton(withTitle: l10n.cpaRollback)
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        cpaUpdating = true
        rebuildMenu()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = self.captureCodexMuxResult(["cpa", "rollback"])
            DispatchQueue.main.async {
                self.cpaUpdating = false
                if result.status != 0 {
                    self.showAlert(self.l10n.cpaRollbackFailed)
                }
                self.refreshStatus()
            }
        }
    }

    @objc private func addDirectRoute() {
        let l10n = self.l10n
        let alert = NSAlert()
        alert.messageText = l10n.directDialogTitle
        alert.informativeText = l10n.directDialogHint

        let stack = NSStackView()
        stack.translatesAutoresizingMaskIntoConstraints = true
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 6

        let baseURLField = NSTextField()
        baseURLField.placeholderString = "https://example.com/v1"
        baseURLField.widthAnchor.constraint(equalToConstant: 320).isActive = true
        let tokenField = NSSecureTextField()
        tokenField.widthAnchor.constraint(equalToConstant: 320).isActive = true
        let modelsField = NSTextField()
        modelsField.placeholderString = "gpt-5.6-sol, gpt-5.6-terra"
        modelsField.widthAnchor.constraint(equalToConstant: 320).isActive = true

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
        // NSAlert uses the accessory view's frame rather than Auto Layout for
        // sizing, so give the stack an explicit size before presenting it.
        let accessorySize = stack.fittingSize
        stack.frame = NSRect(
            x: 0,
            y: 0,
            width: max(accessorySize.width, 478),
            height: accessorySize.height + 12
        )
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
        captureCodexMuxResult(arguments).output
    }

    /// Run codexmux synchronously and return combined output plus exit status.
    private func captureCodexMuxResult(_ arguments: [String]) -> (output: String, status: Int32) {
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
            return ("codexmux is not installed at \(codexmuxURL.path)", 1)
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (String(data: data, encoding: .utf8) ?? "", process.terminationStatus)
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
