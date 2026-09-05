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
    let unifyCompHash: String
    let unifyCompHashFailed: String
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
    let imageModel: String
    let imageDefault: String
    let imageNoModels: String
    let searchBackend: String
    let searchDefault: String
    let searchDisabled: String
    let searchSetFailed: String
    let searchVerifiedOnly: String
    let searchDetect: String
    let searchDetectFailed: String
    let searchDetectRunning: String
    let searchVerified: String
    let searchSupported: String
    let searchUnsupported: String
    let searchUnknown: String
    let searchError: String
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
    let imageSetFailed: String
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
    let directDiscover: String
    let directDiscovering: String
    let directDiscoverFailed: String
    let directMissingFields: String
    let directMissingModels: String
    let directModelsFound: String
    let directNoModels: String
    let directSelectAll: String
    let directClearAll: String
    let directSave: String
    let directManualModels: String
    let directManualPlaceholder: String
    let directTokenPlaceholder: String

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
        unifyCompHash: "Share One Compaction Hash",
        unifyCompHashFailed: "Failed to save the compaction hash setting. See logs.",
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
        imageModel: "Image Generation",
        imageDefault: "Default (official route)",
        imageNoModels: "No CPA image models detected",
        searchBackend: "Shared Web Search",
        searchDefault: "Default (config.toml)",
        searchDisabled: "Disable shared search",
        searchSetFailed: "Failed to set the shared web search backend. See logs.",
        searchVerifiedOnly: "Show verified only",
        searchDetect: "Re-detect search backends…",
        searchDetectFailed: "Failed to detect search backends. See logs.",
        searchDetectRunning: "Detecting search backends…",
        searchVerified: "verified",
        searchSupported: "likely supported",
        searchUnsupported: "unsupported",
        searchUnknown: "not checked",
        searchError: "probe error",
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
        imageSetFailed: "Failed to set the image model. See logs.",
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
        directDialogHint: "Models route as cpa/<slug> straight to this endpoint; CPA is bypassed.",
        directDiscover: "Connect & Fetch Models",
        directDiscovering: "Fetching models…",
        directDiscoverFailed: "Failed to fetch models. Check the URL, token, and network.",
        directMissingFields: "Enter the base URL and token first.",
        directMissingModels: "Select or add at least one model.",
        directModelsFound: "Found %d models",
        directNoModels: "No models found; add them manually.",
        directSelectAll: "Select All",
        directClearAll: "Clear",
        directSave: "Save",
        directManualModels: "Additional models (comma-separated, optional):",
        directManualPlaceholder: "gpt-5.6-sol, gpt-5.6-terra",
        directTokenPlaceholder: "Token",
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
        unifyCompHash: "统一压缩兼容哈希",
        unifyCompHashFailed: "保存压缩哈希设置失败，请查看日志。",
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
        directEndpoints: "直连端点",
        directNoEndpoints: "（暂无直连端点）",
        directAdd: "添加直连端点…",
        directRemove: "移除",
        advanced: "高级功能",
        reviewModel: "审批模型",
        reviewDefault: "默认（官方路由）",
        imageModel: "图像生成",
        imageDefault: "默认（官方路由）",
        imageNoModels: "（未检测到 CPA 图像模型）",
        searchBackend: "共享 Web 搜索",
        searchDefault: "默认（config.toml）",
        searchDisabled: "关闭共享搜索",
        searchSetFailed: "设置共享 Web 搜索后端失败，请查看日志。",
        searchVerifiedOnly: "仅显示已验证",
        searchDetect: "重新检测搜索后端…",
        searchDetectFailed: "检测搜索后端失败，请查看日志。",
        searchDetectRunning: "正在检测搜索后端…",
        searchVerified: "已验证",
        searchSupported: "可能支持",
        searchUnsupported: "不支持",
        searchUnknown: "未检测",
        searchError: "探测失败",
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
        imageSetFailed: "设置图像模型失败，请查看日志。",
        directSetFailed: "保存直连端点失败，请查看日志。",
        quitDialogTitle: "退出 CodexMux？",
        quitDialogBody: "将停止 CodexMux 代理与 CPA 服务，并还原 Codex 配置。",
        quitDialogConfirm: "退出并停止服务",
        quitDialogCancel: "取消",
        alertOK: "好",
        directDialogTitle: "添加直连端点",
        directDialogBaseURL: "基础 URL：",
        directDialogToken: "令牌：",
        directDialogModels: "模型（逗号分隔）：",
        directDialogHint: "模型将以 cpa/<slug> 直连路由到该端点，绕过 CPA。",
        directDiscover: "连接并获取模型",
        directDiscovering: "正在获取模型…",
        directDiscoverFailed: "获取模型失败，请检查地址、令牌和网络。",
        directMissingFields: "请先填写基础 URL 和令牌。",
        directMissingModels: "请至少选择或填写一个模型。",
        directModelsFound: "已发现 %d 个模型",
        directNoModels: "未发现模型，可手动填写。",
        directSelectAll: "全选",
        directClearAll: "清空",
        directSave: "保存",
        directManualModels: "手动补充模型（逗号分隔，可选）：",
        directManualPlaceholder: "gpt-5.6-sol, gpt-5.6-terra",
        directTokenPlaceholder: "令牌",
    )

    /// True when this is the Chinese localization.
    var isChinese: Bool { quit == "退出 CodexMux" }

    var directRemoveItem: String {
        isChinese ? "移除此端点…" : "Remove This Endpoint…"
    }

    var directRouteNoModels: String {
        isChinese ? "（无模型）" : "(no models)"
    }

    var directRemoveDialogTitle: String {
        isChinese ? "移除这个直连端点？" : "Remove this direct endpoint?"
    }

    func directRemoveDialogBody(endpoint: String, models: [String]) -> String {
        let list = models.isEmpty ? "—" : models.map { "cpa/\($0)" }.joined(separator: ", ")
        if isChinese {
            return "将移除 \(endpoint)。它的 \(models.count) 个模型会改回经 CPA 路由：\(list)。\n此操作不可撤销，重新添加需要再次填写 Token。"
        }
        return "Removes \(endpoint). Its \(models.count) model(s) fall back to the CPA route: \(list).\nThis cannot be undone; re-adding it requires the token again."
    }

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
    private var directWindowController: DirectEndpointWindowController?
    private var appUpdateInProgress = false
    private var progressWindow: NSWindow?
    private var progressIndicator: NSProgressIndicator?
    private var searchDetectRunning = false

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
    private var unifyCompHash = true
    private var cpaInstalledVersion: String?
    private var cpaLatestVersion: String?
    private var cpaUpdateAvailable = false
    private var cpaRollbackAvailable = false
    private var cpaUpdating = false
    private var cpaCheckingUpdate = false
    private var activeProfile: String?
    private var savedProfiles: [(name: String, baseURL: String)] = []
    private var reviewOverride: String?
    private var imageOverride: String?
    private var imageModels: [String] = []
    private var searchBackendEnabled: Bool?
    private var searchBackendModel: String?
    private var searchCapabilities: [String: String] = [:]
    private var searchShowVerifiedOnly = UserDefaults.standard.bool(forKey: "searchShowVerifiedOnly")
    private var catalogModels: [String] = []
    private var cpaModels: [String] = []
    private var directRoutes: [(baseURL: String, models: [String])] = []

    private var language: Language {
        Language(rawValue: UserDefaults.standard.string(forKey: languageDefaultsKey) ?? "") ?? .systemPreferred
    }

    private var l10n: L10n { L10n.forLanguage(language) }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Initialize the lazy CLI URL on the main thread before status and
        // controller actions race to access it from background queues.
        _ = codexmuxURL
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
        let managedState = URL(fileURLWithPath: codexmuxHome)
            .appendingPathComponent("state/codex-config.json")
        if FileManager.default.fileExists(atPath: managedState.path) {
            refreshStatus()
        } else {
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
        loadCompHashState { [weak self] enabled in
            self?.unifyCompHash = enabled
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
        loadImageState { [weak self] overrideSlug, models in
            self?.imageOverride = overrideSlug
            self?.imageModels = models
            group.leave()
        }
        group.enter()
        loadSearchState { [weak self] enabled, model in
            self?.searchBackendEnabled = enabled
            self?.searchBackendModel = model
            group.leave()
        }
        group.enter()
        loadCatalogModels { [weak self] models in
            self?.catalogModels = models
            group.leave()
        }
        group.enter()
        loadSearchCapabilities { [weak self] capabilities in
            self?.searchCapabilities = capabilities
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

    /// Load the image route override and the CPA image model list.
    ///
    /// CPA does not advertise image models in its catalog, so the list comes
    /// from a probe and may legitimately be empty; the picker still offers the
    /// official default and whatever slug is currently pinned.
    private func loadImageState(
        _ completion: @escaping (String?, [String]) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let overrideOutput = self.captureCodexMux(["cpa", "image-get"])
            let overrideSlug: String? = {
                let line = overrideOutput.split(separator: "\n").first { $0.hasPrefix("image override: ") }
                guard let line else { return nil }
                let value = line.dropFirst("image override: ".count)
                return value.hasPrefix("(none") ? nil : String(value)
            }()
            let models = self.captureCodexMux(["cpa", "image-model-list"])
                .split(whereSeparator: \.isNewline)
                .map(String.init)
            DispatchQueue.main.async { completion(overrideSlug, models) }
        }
    }

    /// Load the shared web search backend override (background queue only).
    private func loadSearchState(
        _ completion: @escaping (Bool?, String?) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["cpa", "search-get"])
            let line = output.split(separator: "\n").first {
                $0.hasPrefix("shared search:")
            }
            var enabled: Bool?
            var model: String?
            if let line {
                if line.hasPrefix("shared search: enabled: ") {
                    enabled = true
                    model = String(line.dropFirst("shared search: enabled: ".count))
                } else if line.contains("disabled") {
                    enabled = false
                }
            }
            DispatchQueue.main.async { completion(enabled, model) }
        }
    }

    /// Load every merged catalog slug for the search backend submenu.
    private func loadCatalogModels(_ completion: @escaping ([String]) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let models = self.captureCodexMux(["catalog", "models"])
                .split(whereSeparator: \.isNewline)
                .map(String.init)
            DispatchQueue.main.async { completion(models) }
        }
    }

    /// Load cached search capability statuses from `search-capabilities.json`.
    private func loadSearchCapabilities(_ completion: @escaping ([String: String]) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["cpa", "search-capabilities"])
            var capabilities: [String: String] = [:]
            for line in output.split(whereSeparator: \.isNewline) {
                let fields = line.split(separator: " ")
                guard fields.count >= 2 else { continue }
                capabilities[String(fields[0])] = String(fields[1])
            }
            DispatchQueue.main.async { completion(capabilities) }
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

    /// Load whether CodexMux serves one shared `comp_hash` for every model.
    private func loadCompHashState(_ completion: @escaping (Bool) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let output = self.captureCodexMux(["catalog", "comp-hash-get"])
            let enabled = output
                .split(separator: "\n")
                .contains("unify-comp-hash: true")
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
        runCodexMux(["status", "--no-codex-config"]) { output in
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
                // Selecting an endpoint only opens its submenu: browsing the
                // list must never be destructive. Removal lives behind its own
                // item plus a confirmation dialog.
                let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                item.toolTip = "\(route.models.joined(separator: ", ")) → \(route.baseURL)"
                let routeMenu = NSMenu()
                routeMenu.autoenablesItems = false
                let urlRow = NSMenuItem(title: route.baseURL, action: nil, keyEquivalent: "")
                urlRow.isEnabled = false
                routeMenu.addItem(urlRow)
                if route.models.isEmpty {
                    let noModels = NSMenuItem(title: l10n.directRouteNoModels, action: nil, keyEquivalent: "")
                    noModels.isEnabled = false
                    routeMenu.addItem(noModels)
                } else {
                    for model in route.models {
                        let modelRow = NSMenuItem(title: "cpa/\(model)", action: nil, keyEquivalent: "")
                        modelRow.isEnabled = false
                        routeMenu.addItem(modelRow)
                    }
                }
                routeMenu.addItem(.separator())
                let remove = NSMenuItem(title: l10n.directRemoveItem,
                                        action: #selector(removeDirectRoute(_:)),
                                        keyEquivalent: "")
                remove.target = self
                remove.representedObject = route.baseURL
                routeMenu.addItem(remove)
                item.submenu = routeMenu
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
        let compHashItem = NSMenuItem(title: l10n.unifyCompHash,
                                      action: #selector(toggleUnifyCompHash),
                                      keyEquivalent: "")
        compHashItem.target = self
        compHashItem.state = unifyCompHash ? .on : .off
        advancedMenu.addItem(compHashItem)
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

        // Image generation submenu: official by default, or pin a CPA image
        // model. A pinned slug that CPA no longer serves fails on the request
        // itself; the route is never changed automatically.
        let imageItem = NSMenuItem(title: l10n.imageModel, action: nil, keyEquivalent: "")
        let imageMenu = NSMenu()
        imageMenu.autoenablesItems = false

        let imageDefaultItem = NSMenuItem(title: l10n.imageDefault,
                                          action: #selector(selectImageModel(_:)),
                                          keyEquivalent: "")
        imageDefaultItem.target = self
        imageDefaultItem.representedObject = ""
        imageDefaultItem.state = imageOverride == nil ? .on : .off
        imageMenu.addItem(imageDefaultItem)

        // Keep a pinned slug visible even when the probe returns nothing.
        let imageSlugs = (imageModels + [imageOverride].compactMap { $0 }).sorted().unique()
        if imageSlugs.isEmpty {
            let empty = NSMenuItem(title: l10n.imageNoModels, action: nil, keyEquivalent: "")
            empty.isEnabled = false
            imageMenu.addItem(empty)
        }
        for slug in imageSlugs {
            let item = NSMenuItem(title: slug,
                                  action: #selector(selectImageModel(_:)),
                                  keyEquivalent: "")
            item.target = self
            item.representedObject = slug
            item.state = imageOverride == slug ? .on : .off
            imageMenu.addItem(item)
        }
        imageItem.submenu = imageMenu
        menu.addItem(imageItem)

        // Shared web search backend: pick which model executes Responses web_search.
        let searchItem = NSMenuItem(title: l10n.searchBackend, action: nil, keyEquivalent: "")
        let searchMenu = NSMenu()
        searchMenu.autoenablesItems = false

        let searchDefaultItem = NSMenuItem(title: l10n.searchDefault,
                                           action: #selector(selectSearchBackendDefault),
                                           keyEquivalent: "")
        searchDefaultItem.target = self
        searchDefaultItem.state = searchBackendEnabled == nil ? .on : .off
        searchMenu.addItem(searchDefaultItem)

        let searchDisabledItem = NSMenuItem(title: l10n.searchDisabled,
                                            action: #selector(selectSearchBackendDisabled),
                                            keyEquivalent: "")
        searchDisabledItem.target = self
        searchDisabledItem.state = searchBackendEnabled == false ? .on : .off
        searchMenu.addItem(searchDisabledItem)

        let verifiedOnlyItem = NSMenuItem(title: l10n.searchVerifiedOnly,
                                          action: #selector(toggleVerifiedOnly),
                                          keyEquivalent: "")
        verifiedOnlyItem.target = self
        verifiedOnlyItem.state = searchShowVerifiedOnly ? .on : .off
        searchMenu.addItem(verifiedOnlyItem)

        let detectItem = NSMenuItem(title: l10n.searchDetect,
                                    action: #selector(detectSearchBackends),
                                    keyEquivalent: "")
        detectItem.target = self
        searchMenu.addItem(detectItem)
        searchMenu.addItem(.separator())

        let searchSlugs = catalogModels
            .filter { $0 != "codex-auto-review" && !$0.hasSuffix("/codex-auto-review") }
            .sorted()
            .unique()
        for slug in searchSlugs {
            let status = searchCapabilities[slug] ?? "unknown"
            if searchShowVerifiedOnly && !["verified", "supported"].contains(status) {
                continue
            }
            let statusLabel: String
            switch status {
            case "verified": statusLabel = l10n.searchVerified
            case "supported": statusLabel = l10n.searchSupported
            case "unsupported": statusLabel = l10n.searchUnsupported
            case "error": statusLabel = l10n.searchError
            default: statusLabel = l10n.searchUnknown
            }
            let item = NSMenuItem(title: slug,
                                  action: #selector(selectSearchBackendModel(_:)),
                                  keyEquivalent: "")
            item.target = self
            item.title = "\(slug)  (\(statusLabel))"
            item.representedObject = slug
            item.state = searchBackendEnabled == true && searchBackendModel == slug ? .on : .off
            item.toolTip = statusLabel
            searchMenu.addItem(item)
        }
        searchItem.submenu = searchMenu
        menu.addItem(searchItem)

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

    @objc private func toggleUnifyCompHash(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["catalog", "comp-hash-set", enabled ? "true" : "false"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.unifyCompHashFailed ?? "")
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
        let controller = directWindowController ?? DirectEndpointWindowController()
        controller.configure(
            l10n: l10n,
            onDiscover: { [weak self, weak controller] baseURL, token in
                self?.discoverDirectModels(baseURL: baseURL, token: token, controller: controller)
            },
            onSave: { [weak self] baseURL, token, models, completion in
                self?.saveDirectRoute(
                    baseURL: baseURL,
                    token: token,
                    models: models,
                    completion: completion
                )
            }
        )
        directWindowController = controller
        controller.present()
    }

    private func discoverDirectModels(
        baseURL: String,
        token: String,
        controller: DirectEndpointWindowController?
    ) {
        guard let controller else { return }
        var environment = codexMuxEnvironment
        environment["CODEXMUX_DIRECT_TOKEN"] = token
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = self.captureCodexMuxResult(
                ["cpa", "direct-discover", "--base-url", baseURL],
                environment: environment
            )
            DispatchQueue.main.async {
                controller.setDiscovering(false)
                if result.status == 0 {
                    let models = result.output
                        .split(whereSeparator: \.isNewline)
                        .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
                        .filter { !$0.isEmpty }
                    controller.setModels(models)
                } else {
                    let detail = result.output.trimmingCharacters(in: .whitespacesAndNewlines)
                    controller.setError(
                        detail.isEmpty
                            ? self.l10n.directDiscoverFailed
                            : "\(self.l10n.directDiscoverFailed)\n\(detail)"
                    )
                }
            }
        }
    }

    private func saveDirectRoute(
        baseURL: String,
        token: String,
        models: [String],
        completion: @escaping (Bool) -> Void
    ) {
        var environment = codexMuxEnvironment
        environment["CODEXMUX_DIRECT_TOKEN"] = token
        runCodexMuxDetached(
            ["cpa", "direct-add", models.joined(separator: ","), "--base-url", baseURL],
            environment: environment
        ) { [weak self] ok in
            completion(ok)
            self?.refreshStatus()
        }
    }

    @objc private func removeDirectRoute(_ sender: NSMenuItem) {
        guard let baseURL = sender.representedObject as? String else { return }
        // Removing a direct endpoint silently reroutes all of its models
        // through CPA, so it always requires an explicit confirmation.
        let models = directRoutes.first { $0.baseURL == baseURL }?.models ?? []
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = l10n.directRemoveDialogTitle
        alert.informativeText = l10n.directRemoveDialogBody(endpoint: baseURL, models: models)
        alert.addButton(withTitle: l10n.directRemove)
        alert.addButton(withTitle: l10n.quitDialogCancel)
        guard alert.runModal() == .alertFirstButtonReturn else { return }
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

    @objc private func selectImageModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "image-set", slug]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.imageSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func selectSearchBackendDefault(_ sender: NSMenuItem) {
        runCodexMuxDetached(["cpa", "search-set", "default"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.searchSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func selectSearchBackendDisabled(_ sender: NSMenuItem) {
        runCodexMuxDetached(["cpa", "search-set", "off"]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.searchSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func selectSearchBackendModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "search-set", slug]) { [weak self] ok in
            if !ok {
                self?.showAlert(self?.l10n.searchSetFailed ?? "")
            }
            self?.refreshStatus()
        }
    }

    @objc private func toggleVerifiedOnly(_ sender: NSMenuItem) {
        searchShowVerifiedOnly = sender.state != .on
        UserDefaults.standard.set(searchShowVerifiedOnly, forKey: "searchShowVerifiedOnly")
        rebuildMenu()
    }

    @objc private func detectSearchBackends() {
        guard !searchDetectRunning else { return }
        searchDetectRunning = true
        runCodexMuxDetachedWithProgress(
            ["cpa", "search-detect"],
            progressText: l10n.searchDetectRunning
        ) { [weak self] ok in
            guard let self else { return }
            self.searchDetectRunning = false
            if !ok {
                self.showAlert(self.l10n.searchDetectFailed)
            }
            self.refreshStatus()
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
    private func captureCodexMuxResult(
        _ arguments: [String],
        environment: [String: String]? = nil
    ) -> (output: String, status: Int32) {
        let process = Process()
        process.executableURL = codexmuxURL
        process.arguments = arguments
        process.environment = environment ?? codexMuxEnvironment
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

    /// Run a CLI action with a small floating progress window so long-running
    /// work stays visible after the status menu collapses.
    private func runCodexMuxDetachedWithProgress(
        _ arguments: [String],
        progressText: String,
        environment: [String: String]? = nil,
        completion: @escaping (Bool) -> Void
    ) {
        DispatchQueue.main.async { self.showProgress(progressText) }
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
                DispatchQueue.main.async {
                    self.hideProgress()
                    completion(false)
                }
                return
            }
            _ = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            let ok = process.terminationStatus == 0
            DispatchQueue.main.async {
                self.hideProgress()
                completion(ok)
            }
        }
    }

    private func showProgress(_ text: String) {
        hideProgress()
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 360, height: 112),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.title = ""
        window.isReleasedWhenClosed = false
        window.level = .floating
        window.center()
        guard let content = window.contentView else { return }
        let root = NSStackView()
        root.orientation = .vertical
        root.alignment = .centerX
        root.spacing = 12
        root.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(root)
        NSLayoutConstraint.activate([
            root.centerXAnchor.constraint(equalTo: content.centerXAnchor),
            root.centerYAnchor.constraint(equalTo: content.centerYAnchor),
        ])
        let indicator = NSProgressIndicator()
        indicator.style = .spinning
        indicator.controlSize = .regular
        indicator.isIndeterminate = true
        indicator.startAnimation(nil)
        root.addArrangedSubview(indicator)
        let label = NSTextField(labelWithString: text)
        label.alignment = .center
        label.lineBreakMode = .byWordWrapping
        label.maximumNumberOfLines = 2
        label.translatesAutoresizingMaskIntoConstraints = false
        label.widthAnchor.constraint(equalToConstant: 300).isActive = true
        root.addArrangedSubview(label)
        window.orderFrontRegardless()
        progressWindow = window
        progressIndicator = indicator
    }

    private func hideProgress() {
        progressIndicator?.stopAnimation(nil)
        progressIndicator = nil
        progressWindow?.orderOut(nil)
        progressWindow = nil
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
