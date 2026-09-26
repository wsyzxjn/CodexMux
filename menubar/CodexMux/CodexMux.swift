import Cocoa

/// User-facing strings for every language CodexMux supports.
struct L10n {
    let isChinese: Bool
    let codexmuxStatusRunning: String
    let codexmuxStatusIdle: String
    let codexmuxStatusStopped: String
    let cpaStatusRunning: String
    let cpaStatusStopped: String
    let cpaStatusNotInstalled: String
    let cpaStatusRemote: String
    let services: String
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
    let cpaModelsLoading: String
    let openLogs: String
    let openCpaManagement: String
    let copyCpaManagementKey: String
    let profiles: String
    let profileNoProfiles: String
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
    let appUpdateDownloading: String
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
    let quitDialogTitle: String
    let quitDialogBody: String
    let quitDialogConfirm: String
    let quitDialogCancel: String
    let quitFailedTitle: String
    let quitAnyway: String
    let quitting: String
    let alertOK: String

    static let english = L10n(
        isChinese: false,
        codexmuxStatusRunning: "CodexMux: running",
        codexmuxStatusIdle: "CodexMux: idle",
        codexmuxStatusStopped: "CodexMux: stopped",
        cpaStatusRunning: "CPA service: running",
        cpaStatusStopped: "CPA service: stopped",
        cpaStatusNotInstalled: "CPA: not installed",
        cpaStatusRemote: "CPA: remote endpoint",
        services: "Services",
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
        cpaModelsLoading: "Loading CPA models…",
        openLogs: "Open Logs Folder",
        openCpaManagement: "Open CPA Web Management",
        copyCpaManagementKey: "Copy CPA Management Key",
        profiles: "CPA Profiles",
        profileNoProfiles: "No saved profiles",
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
        appUpdateDownloading: "Downloading and verifying update…",
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
        quitDialogTitle: "Quit CodexMux?",
        quitDialogBody: "This stops the CodexMux proxy and the CPA service, and restores the Codex configuration.",
        quitDialogConfirm: "Quit and Stop Services",
        quitDialogCancel: "Cancel",
        quitFailedTitle: "Failed to stop CodexMux and restore the Codex configuration.",
        quitAnyway: "Quit Anyway",
        quitting: "Stopping CodexMux services…",
        alertOK: "OK"
    )

    static let chinese = L10n(
        isChinese: true,
        codexmuxStatusRunning: "CodexMux：运行中",
        codexmuxStatusIdle: "CodexMux：待机",
        codexmuxStatusStopped: "CodexMux：已停止",
        cpaStatusRunning: "CPA 服务：运行中",
        cpaStatusStopped: "CPA 服务：已停止",
        cpaStatusNotInstalled: "CPA：未安装",
        cpaStatusRemote: "CPA：远程端点",
        services: "服务控制",
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
        cpaModelsLoading: "正在加载 CPA 模型…",
        openLogs: "打开日志文件夹",
        openCpaManagement: "打开 CPA Web 管理",
        copyCpaManagementKey: "复制 CPA 管理密钥",
        profiles: "CPA 配置",
        profileNoProfiles: "（暂无保存的配置）",
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
        appUpdateDownloading: "正在下载并校验更新…",
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
        quitDialogTitle: "退出 CodexMux？",
        quitDialogBody: "将停止 CodexMux 代理与 CPA 服务，并还原 Codex 配置。",
        quitDialogConfirm: "退出并停止服务",
        quitDialogCancel: "取消",
        quitFailedTitle: "停止 CodexMux 并还原 Codex 配置失败。",
        quitAnyway: "仍然退出",
        quitting: "正在停止 CodexMux 服务…",
        alertOK: "好"
    )

    func stateUnavailable(_ detail: String) -> String {
        isChinese ? "无法读取状态：\(detail)" : "Status unavailable: \(detail)"
    }

    func cpaVersion(_ version: String) -> String {
        isChinese ? "版本：\(version)" : "Version: \(version)"
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

    func searchDefaultFollowing(_ model: String) -> String {
        isChinese ? "默认（config.toml：\(model)）" : "Default (config.toml: \(model))"
    }

    func searchCapabilityLabel(_ capability: MenubarState.SearchCapability) -> String {
        switch capability {
        case .verified: return searchVerified
        case .supported: return searchSupported
        case .unsupported: return searchUnsupported
        case .unknown: return searchUnknown
        case .error: return searchError
        }
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

/// A submenu that rebuilds its items from current state each time it is about
/// to open. Data that arrives while the status menu is open then still shows
/// up, without rebuilding (and collapsing) the root menu.
private final class StateMenu: NSMenu {
    private let build: (NSMenu) -> Void

    init(delegate: NSMenuDelegate, build: @escaping (NSMenu) -> Void) {
        self.build = build
        super.init(title: "")
        autoenablesItems = false
        self.delegate = delegate
        reload()
    }

    @available(*, unavailable)
    required init(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func reload() {
        removeAllItems()
        build(self)
    }
}

/// Menu bar controller for CodexMux: service status, start/stop, CPA
/// management, and log access.
///
/// General model selection stays in Codex; the routing controls here are the
/// explicit review, image, and shared web search overrides. Everything local
/// comes from one `codexmux menubar-state` call. The CPA model lists need
/// requests to CPA itself, so they are fetched only when the menu opens with a
/// stale cache or after an action that changes CPA.
final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private static let repositoryURL = URL(string: "https://github.com/wsyzxjn/CodexMux")!
    private static let statusRefreshInterval: TimeInterval = 10
    private static let cpaModelListTTL: TimeInterval = 60
    /// Background reads running longer than this are stopped, so one stuck
    /// CLI call cannot hold off every later refresh.
    private static let backgroundCommandTimeout: TimeInterval = 30

    private var statusItem: NSStatusItem!
    private var menu: NSMenu!
    private var timer: Timer?
    private let appUpdater = AppUpdater(repository: "wsyzxjn/CodexMux")
    private var aboutWindowController: AboutWindowController?
    private var progressWindow: NSWindow?
    private var progressIndicator: NSProgressIndicator?

    /// Rebuilding the root menu while it is open collapses the submenu being
    /// browsed, so rebuilds and alerts wait until it closes.
    private var menuIsOpen = false
    private var menuNeedsRebuild = false
    private var deferredPresentations: [() -> Void] = []
    private weak var proxyStatusMenuItem: NSMenuItem?
    private weak var stateErrorMenuItem: NSMenuItem?
    /// Root items that need local state; disabled while none is available.
    private var stateDependentMenuItems: [NSMenuItem] = []

    /// The last successful `menubar-state` result; nil when the call failed.
    private var state: MenubarState?
    /// Why the last `menubar-state` call failed.
    private var stateError: String?
    private var stateRefreshInFlight = false
    private var stateRefreshQueued = false

    private var cpaModels: [String] = []
    private var imageModels: [String] = []
    private var cpaModelsLoading = false
    private var imageModelsLoading = false
    private var cpaModelListsFetchedAt: Date?
    /// Identifies the newest list fetch; older results are discarded.
    private var cpaModelListsGeneration = 0

    private var appUpdateInProgress = false
    private var cpaInstalling = false
    private var cpaUpdating = false
    private var cpaCheckingUpdate = false
    private var cpaLatestVersion: String?
    private var cpaUpdateAvailable = false
    private var searchDetectRunning = false
    private var quitInProgress = false
    private var searchShowVerifiedOnly = UserDefaults.standard.bool(forKey: "searchShowVerifiedOnly")
    private let languageDefaultsKey = "language"

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

    private var language: Language {
        Language(rawValue: UserDefaults.standard.string(forKey: languageDefaultsKey) ?? "") ?? .systemPreferred
    }

    private var l10n: L10n { L10n.forLanguage(language) }

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Initialize the lazy CLI URL on the main thread before status and
        // controller actions race to access it from background queues.
        _ = codexmuxURL
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        menu = NSMenu()
        menu.autoenablesItems = false
        menu.delegate = self
        statusItem.menu = menu
        updateIcon()
        rebuildMenu()
        refreshStatus()
        // The menu bar app is the controller: opening it brings the proxy up
        // and enables the managed Codex configuration from this GUI context
        // (LaunchAgent serve uses --no-codex-config because of TCC). launchd
        // then wakes the proxy and an enabled local CPA only when Codex
        // Desktop or CLI actually connects.
        let managedState = URL(fileURLWithPath: codexmuxHome)
            .appendingPathComponent("state/codex-config.json")
        if !FileManager.default.fileExists(atPath: managedState.path) {
            runCodexMuxDetached(["install"]) { [weak self] result in
                guard let self else { return }
                if result.succeeded {
                    self.relaunchRunningCodex()
                } else {
                    self.reportFailure(result, \.restartFailed)
                }
            }
        }
        let timer = Timer.scheduledTimer(
            withTimeInterval: Self.statusRefreshInterval,
            repeats: true
        ) { [weak self] _ in
            self?.refreshStatus()
        }
        timer.tolerance = 2
        self.timer = timer
    }

    func applicationWillTerminate(_ notification: Notification) {
        timer?.invalidate()
    }

    // MARK: - Menu delegate

    func menuWillOpen(_ menu: NSMenu) {
        guard menu === self.menu else { return }
        menuIsOpen = true
        refreshStatus()
        refreshCPAModelListsIfStale()
    }

    func menuDidClose(_ menu: NSMenu) {
        guard menu === self.menu else { return }
        menuIsOpen = false
        // The chosen item's action may not have been sent yet; rebuilding now
        // could remove that item first, so wait for the next main-queue turn.
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.menuIsOpen else { return }
            if self.menuNeedsRebuild {
                self.rebuildMenu()
            }
            let presentations = self.deferredPresentations
            self.deferredPresentations.removeAll()
            presentations.forEach { $0() }
        }
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        (menu as? StateMenu)?.reload()
    }

    // MARK: - Status

    /// Refresh the local state from one `codexmux menubar-state` call. Calls
    /// made while one is running collapse into a single follow-up refresh.
    private func refreshStatus() {
        guard !stateRefreshInFlight else {
            stateRefreshQueued = true
            return
        }
        stateRefreshInFlight = true
        DispatchQueue.global(qos: .utility).async {
            let (state, error) = self.loadState()
            DispatchQueue.main.async {
                self.stateRefreshInFlight = false
                self.applyState(state, error: error)
                if self.stateRefreshQueued {
                    self.stateRefreshQueued = false
                    self.refreshStatus()
                }
            }
        }
    }

    /// Run `menubar-state` (background queue only). Its stdout must be exactly
    /// one JSON object; any failure is reported instead of guessing.
    private func loadState() -> (MenubarState?, String?) {
        let result = runCodexMux(["menubar-state"], timeout: Self.backgroundCommandTimeout)
        guard result.succeeded else {
            // Standard error carries the reason; stdout is only JSON data.
            let detail = [result.stderr, result.stdout]
                .map { CommandResult.tail(of: $0) }
                .first { !$0.isEmpty }
            return (nil, detail ?? "exit status \(result.status)")
        }
        do {
            return (try MenubarState.decode(Data(result.stdout.utf8)), nil)
        } catch {
            return (nil, MenubarState.describe(error))
        }
    }

    private func applyState(_ newState: MenubarState?, error: String?) {
        if newState != state || error != stateError {
            state = newState
            stateError = error
            updateIcon()
            requestMenuRebuild()
        }
        // State that arrives while the menu is open may show CPA just came up.
        if menuIsOpen {
            refreshCPAModelListsIfStale()
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

    private func updateIcon() {
        guard let button = statusItem.button else { return }
        let label: String
        switch state?.proxy {
        case .running: label = "CodexMux"
        case .idle: label = "CodexMux idle"
        case .notInstalled, nil: label = "CodexMux stopped"
        }
        button.image = nil
        button.title = ">_<"
        button.font = .monospacedSystemFont(ofSize: 13, weight: .semibold)
        button.toolTip = label
        button.setAccessibilityLabel(label)
    }

    // MARK: - CPA model lists

    /// Refetch the CPA model lists once they are older than the TTL. Called
    /// only while the menu is open: the lists need requests to CPA itself.
    private func refreshCPAModelListsIfStale() {
        guard let cpa = state?.cpa else { return }
        // A local CPA must be running. Whether a remote endpoint is reachable
        // is not part of the local state, so it is simply tried.
        guard cpa.running || !cpa.local else { return }
        if let fetchedAt = cpaModelListsFetchedAt,
           Date().timeIntervalSince(fetchedAt) < Self.cpaModelListTTL {
            return
        }
        refreshCPAModelLists()
    }

    /// Fetch both CPA model lists now, superseding any fetch still running.
    /// A failed fetch keeps the previous list.
    private func refreshCPAModelLists() {
        cpaModelListsGeneration += 1
        let generation = cpaModelListsGeneration
        cpaModelListsFetchedAt = Date()
        cpaModelsLoading = true
        imageModelsLoading = true
        runCodexMuxInBackground(
            ["cpa", "model-list"],
            timeout: Self.backgroundCommandTimeout
        ) { [weak self] result in
            guard let self, generation == self.cpaModelListsGeneration else { return }
            self.cpaModelsLoading = false
            if result.succeeded {
                self.cpaModels = result.stdoutLines
            }
        }
        // CPA lists no image models, so this is a best-effort probe whose
        // result may legitimately be empty; the picker still offers the
        // official default and whatever slug is currently pinned.
        runCodexMuxInBackground(
            ["cpa", "image-model-list"],
            timeout: Self.backgroundCommandTimeout
        ) { [weak self] result in
            guard let self, generation == self.cpaModelListsGeneration else { return }
            self.imageModelsLoading = false
            if result.succeeded {
                self.imageModels = result.stdoutLines
            }
        }
    }

    // MARK: - Menu

    /// Rebuild the root menu now, or once it closes if it is open. While it is
    /// open only the status rows are updated in place.
    private func requestMenuRebuild() {
        if menuIsOpen {
            menuNeedsRebuild = true
            updateLiveMenuItems()
        } else {
            rebuildMenu()
        }
    }

    private func rebuildMenu() {
        menuNeedsRebuild = false
        let l10n = self.l10n
        menu.removeAllItems()

        let statusRow = infoItem("")
        proxyStatusMenuItem = statusRow
        menu.addItem(statusRow)
        let errorRow = infoItem("")
        errorRow.image = NSImage(
            systemSymbolName: "exclamationmark.triangle",
            accessibilityDescription: nil
        )
        stateErrorMenuItem = errorRow
        menu.addItem(errorRow)

        menu.addItem(submenuItem(l10n.services) { [unowned self] in self.buildServicesMenu($0) })
        stateDependentMenuItems = [
            submenuItem("CPA") { [unowned self] in self.buildCPAMenu($0) },
            submenuItem(l10n.advanced) { [unowned self] in self.buildAdvancedMenu($0) },
            submenuItem(l10n.reviewModel) { [unowned self] in self.buildReviewMenu($0) },
            submenuItem(l10n.imageModel) { [unowned self] in self.buildImageMenu($0) },
            submenuItem(l10n.searchBackend) { [unowned self] in self.buildSearchMenu($0) },
        ]
        for item in stateDependentMenuItems {
            menu.addItem(item)
        }
        menu.addItem(actionItem(l10n.openLogs, #selector(openLogs), key: "l"))
        menu.addItem(submenuItem(l10n.language) { [unowned self] in self.buildLanguageMenu($0) })

        menu.addItem(.separator())
        menu.addItem(actionItem(l10n.about, #selector(showAbout)))
        menu.addItem(actionItem(
            appUpdateInProgress ? l10n.checkingAppUpdates : l10n.checkAppUpdates,
            #selector(checkAppUpdates),
            enabled: !appUpdateInProgress
        ))

        menu.addItem(.separator())
        menu.addItem(actionItem(l10n.quit, #selector(confirmQuit), key: "q", enabled: !quitInProgress))
        updateLiveMenuItems()
    }

    /// Update the rows that may change while the menu is open: the proxy
    /// status, the error row, and whether state-backed submenus can open.
    private func updateLiveMenuItems() {
        let l10n = self.l10n
        switch state?.proxy {
        case .running: proxyStatusMenuItem?.title = l10n.codexmuxStatusRunning
        case .idle: proxyStatusMenuItem?.title = l10n.codexmuxStatusIdle
        case .notInstalled, nil: proxyStatusMenuItem?.title = l10n.codexmuxStatusStopped
        }
        let error = stateError.map(l10n.stateUnavailable) ?? state?.errors.first
        if let errorRow = stateErrorMenuItem {
            errorRow.isHidden = error == nil
            errorRow.title = error.map { Self.singleLine($0, limit: 80) } ?? ""
            errorRow.toolTip = error
        }
        for item in stateDependentMenuItems {
            item.isEnabled = state != nil
        }
    }

    private func buildServicesMenu(_ menu: NSMenu) {
        let l10n = self.l10n
        menu.addItem(actionItem(l10n.restartCodexMux, #selector(restartProxy), key: "r"))
        menu.addItem(actionItem(
            l10n.stopCodexMux,
            #selector(stopProxy),
            key: "s",
            enabled: state?.proxy != .notInstalled
        ))
    }

    /// One CPA menu owns installation, lifecycle, updates, profiles, and web
    /// management. Lifecycle, startup, and update controls manage the local
    /// CLIProxyAPI, so they only apply while the endpoint is on this machine.
    private func buildCPAMenu(_ menu: NSMenu) {
        guard let cpa = state?.cpa else { return }
        let l10n = self.l10n
        let status: String
        if !cpa.local {
            status = l10n.cpaStatusRemote
        } else if !cpa.installed {
            status = l10n.cpaStatusNotInstalled
        } else {
            status = cpa.running ? l10n.cpaStatusRunning : l10n.cpaStatusStopped
        }
        menu.addItem(infoItem(status))
        menu.addItem(.separator())

        let managesLocalCPA = cpa.local && cpa.installed
        if cpa.installed {
            menu.addItem(actionItem(l10n.startCPA, #selector(startCPA),
                                    enabled: managesLocalCPA && !cpa.running))
            menu.addItem(actionItem(l10n.stopCPA, #selector(stopCPA),
                                    enabled: managesLocalCPA && cpa.running))
        } else {
            menu.addItem(actionItem(cpaInstalling ? l10n.installingCPA : l10n.installCPA,
                                    #selector(installCPA),
                                    enabled: cpa.local && !cpaInstalling))
        }
        menu.addItem(actionItem(l10n.cpaAutostart, #selector(toggleCPAAutostart(_:)),
                                enabled: managesLocalCPA, checked: cpa.autostart))
        menu.addItem(.separator())

        let update = submenuItem(l10n.cpaUpdate) { [unowned self] in self.buildCPAUpdateMenu($0) }
        update.isEnabled = managesLocalCPA
        menu.addItem(update)
        menu.addItem(submenuItem(l10n.profiles) { [unowned self] in self.buildProfilesMenu($0) })

        menu.addItem(.separator())
        // A remote endpoint serves its own management page; the key belongs
        // to the local CPA only.
        menu.addItem(actionItem(l10n.openCpaManagement, #selector(openCpaManagement),
                                enabled: !cpa.local || cpa.installed))
        menu.addItem(actionItem(l10n.copyCpaManagementKey, #selector(copyCpaManagementKey),
                                enabled: managesLocalCPA))
    }

    /// Check, apply, and roll back the managed local CLIProxyAPI release.
    private func buildCPAUpdateMenu(_ menu: NSMenu) {
        guard let cpa = state?.cpa else { return }
        let l10n = self.l10n
        menu.addItem(infoItem(cpa.version.map(l10n.cpaVersion) ?? l10n.cpaVersionUnknown))
        if let latest = cpaLatestVersion {
            menu.addItem(infoItem(cpaUpdateAvailable
                ? l10n.cpaUpdateAvailable(latest)
                : l10n.cpaUpToDate(latest)))
        }
        menu.addItem(.separator())
        menu.addItem(actionItem(l10n.cpaCheckUpdate, #selector(checkCpaUpdate),
                                enabled: !cpaUpdating && !cpaCheckingUpdate))
        if let latest = cpaLatestVersion, cpaUpdateAvailable {
            menu.addItem(actionItem(l10n.cpaUpdateTo(latest), #selector(updateCpa),
                                    enabled: !cpaUpdating))
        }
        if cpa.rollbackAvailable {
            menu.addItem(actionItem(l10n.cpaRollback, #selector(rollbackCpa),
                                    enabled: !cpaUpdating))
        }
    }

    /// Click a saved endpoint to validate and switch to it.
    private func buildProfilesMenu(_ menu: NSMenu) {
        guard let profiles = state?.profiles else { return }
        guard !profiles.saved.isEmpty else {
            menu.addItem(infoItem(l10n.profileNoProfiles))
            return
        }
        for profile in profiles.saved {
            let item = actionItem(profile.name, #selector(switchProfile(_:)),
                                  checked: profiles.active == profile.name,
                                  value: profile.name)
            item.toolTip = profile.baseURL
            menu.addItem(item)
        }
    }

    /// Catalog settings that affect how Codex sees merged models.
    private func buildAdvancedMenu(_ menu: NSMenu) {
        guard let catalog = state?.catalog else { return }
        let l10n = self.l10n
        menu.addItem(actionItem(l10n.advertiseUltra, #selector(toggleAdvertiseUltra(_:)),
                                checked: catalog.advertiseUltra))
        menu.addItem(actionItem(l10n.unifyCompHash, #selector(toggleUnifyCompHash(_:)),
                                checked: catalog.unifyCompHash))
    }

    /// Pick which CPA model handles codex-auto-review.
    private func buildReviewMenu(_ menu: NSMenu) {
        let l10n = self.l10n
        let selected = state?.reviewOverride
        menu.addItem(actionItem(l10n.reviewDefault, #selector(selectReviewModel(_:)),
                                checked: selected == nil, value: ""))
        // Keep a pinned slug visible even before the CPA list has loaded.
        let slugs = (cpaModels + [selected].compactMap { $0 }).sorted().unique()
        if slugs.isEmpty && cpaModelsLoading {
            menu.addItem(infoItem(l10n.cpaModelsLoading))
        }
        for slug in slugs {
            menu.addItem(actionItem(slug, #selector(selectReviewModel(_:)),
                                    checked: selected == slug, value: slug))
        }
    }

    /// Official by default, or pin a CPA image model. A pinned slug that CPA
    /// no longer serves fails on the request itself; the route is never
    /// changed automatically.
    private func buildImageMenu(_ menu: NSMenu) {
        let l10n = self.l10n
        let selected = state?.imageOverride
        menu.addItem(actionItem(l10n.imageDefault, #selector(selectImageModel(_:)),
                                checked: selected == nil, value: ""))
        // Keep a pinned slug visible even when the probe returns nothing.
        let slugs = (imageModels + [selected].compactMap { $0 }).sorted().unique()
        if slugs.isEmpty {
            menu.addItem(infoItem(imageModelsLoading ? l10n.cpaModelsLoading : l10n.imageNoModels))
        }
        for slug in slugs {
            menu.addItem(actionItem(slug, #selector(selectImageModel(_:)),
                                    checked: selected == slug, value: slug))
        }
    }

    /// Pick which model executes the shared Responses web_search.
    private func buildSearchMenu(_ menu: NSMenu) {
        guard let state else { return }
        let l10n = self.l10n
        let search = state.search
        menu.addItem(actionItem(
            search.defaultBackendModel.map(l10n.searchDefaultFollowing) ?? l10n.searchDefault,
            #selector(selectSearchBackendDefault),
            checked: search.mode == .default
        ))
        menu.addItem(actionItem(l10n.searchDisabled, #selector(selectSearchBackendDisabled),
                                checked: search.mode == .disabled))
        menu.addItem(actionItem(l10n.searchVerifiedOnly, #selector(toggleVerifiedOnly(_:)),
                                checked: searchShowVerifiedOnly))
        menu.addItem(actionItem(searchDetectRunning ? l10n.searchDetectRunning : l10n.searchDetect,
                                #selector(detectSearchBackends),
                                enabled: !searchDetectRunning))
        menu.addItem(.separator())

        let selected = search.mode == .enabled ? search.backendModel : nil
        let slugs = (state.catalog.models + [selected].compactMap { $0 })
            .filter { $0 != "codex-auto-review" && !$0.hasSuffix("/codex-auto-review") }
            .sorted()
            .unique()
        for slug in slugs {
            let capability = state.searchCapabilities[slug] ?? .unknown
            // The selected backend stays visible even when the filter is on.
            if searchShowVerifiedOnly && slug != selected
                && ![.verified, .supported].contains(capability) {
                continue
            }
            let label = l10n.searchCapabilityLabel(capability)
            let item = actionItem("\(slug)  (\(label))", #selector(selectSearchBackendModel(_:)),
                                  checked: slug == selected, value: slug)
            item.toolTip = label
            menu.addItem(item)
        }
    }

    private func buildLanguageMenu(_ menu: NSMenu) {
        for option in Language.allCases {
            menu.addItem(actionItem(option.displayName, #selector(selectLanguage(_:)),
                                    checked: option == language, value: option.rawValue))
        }
    }

    private func infoItem(_ title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }

    private func actionItem(
        _ title: String,
        _ action: Selector,
        key: String = "",
        enabled: Bool = true,
        checked: Bool = false,
        value: Any? = nil
    ) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.target = self
        item.isEnabled = enabled
        item.state = checked ? .on : .off
        item.representedObject = value
        return item
    }

    private func submenuItem(_ title: String, build: @escaping (NSMenu) -> Void) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.submenu = StateMenu(delegate: self, build: build)
        return item
    }

    /// Collapse `text` to one line of at most `limit` characters.
    private static func singleLine(_ text: String, limit: Int) -> String {
        let line = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return line.count > limit ? String(line.prefix(limit - 1)) + "…" : line
    }

    // MARK: - Actions

    @objc private func showAbout() {
        let controller = aboutWindowController ?? AboutWindowController(
            repositoryURL: Self.repositoryURL
        )
        controller.configure(
            appVersion: appVersion,
            cliVersion: state?.version ?? "…",
            l10n: l10n,
            isChecking: appUpdateInProgress,
            onCheckForUpdates: { [weak self] in self?.beginAppUpdateCheck() }
        )
        aboutWindowController = controller
        controller.showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
        if state == nil {
            // The state call is failing; still identify the bundled CLI.
            runCodexMuxInBackground(
                ["--version"],
                timeout: Self.backgroundCommandTimeout
            ) { [weak controller] result in
                let version = result.succeeded
                    ? result.stdout.split(whereSeparator: \.isWhitespace).last.map(String.init)
                    : nil
                controller?.setCLIVersion(version ?? "unknown")
            }
        }
    }

    @objc private func checkAppUpdates() {
        beginAppUpdateCheck()
    }

    private var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
            ?? "unknown"
    }

    private func beginAppUpdateCheck() {
        guard !appUpdateInProgress else { return }
        appUpdateInProgress = true
        aboutWindowController?.setUpdateState(
            message: l10n.checkingAppUpdates,
            isChecking: true
        )
        requestMenuRebuild()

        appUpdater.checkForUpdate(currentVersion: appVersion) { [weak self] result in
            DispatchQueue.main.async {
                guard let self else { return }
                self.appUpdateInProgress = false
                self.requestMenuRebuild()
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
                    self.presentWhenMenuClosed { self.confirmAppUpdate(release) }
                case .failure(let error):
                    self.aboutWindowController?.setUpdateState(
                        message: self.l10n.appUpdateCheckFailed,
                        isChecking: false
                    )
                    self.showAlert(self.l10n.appUpdateCheckFailed, detail: Self.describe(error))
                }
            }
        }
    }

    private func confirmAppUpdate(_ release: AppRelease) {
        let l10n = self.l10n
        let choice = runAlert(
            title: l10n.appUpdateDialogTitle,
            body: l10n.appUpdateDialogBody(from: appVersion, to: release.version),
            buttons: [l10n.appUpdateDialogInstall, l10n.quitDialogCancel]
        )
        guard choice == .alertFirstButtonReturn else {
            aboutWindowController?.setUpdateState(
                message: l10n.appUpdateAvailable(release.version),
                isChecking: false
            )
            return
        }

        appUpdateInProgress = true
        aboutWindowController?.setUpdateState(
            message: l10n.appUpdateDownloading,
            isChecking: true
        )
        requestMenuRebuild()
        appUpdater.prepareUpdate(release) { [weak self] result in
            DispatchQueue.main.async {
                guard let self else { return }
                switch result {
                case .success(let prepared):
                    do {
                        try self.appUpdater.installAndRelaunch(prepared)
                    } catch {
                        self.finishFailedAppUpdate(error)
                    }
                case .failure(let error):
                    self.finishFailedAppUpdate(error)
                }
            }
        }
    }

    private func finishFailedAppUpdate(_ error: Error) {
        appUpdateInProgress = false
        requestMenuRebuild()
        aboutWindowController?.setUpdateState(
            message: l10n.appUpdateFailed,
            isChecking: false
        )
        showAlert(l10n.appUpdateFailed, detail: Self.describe(error))
    }

    /// A short technical reason for an updater failure.
    private static func describe(_ error: Error) -> String {
        error is AppUpdaterError ? String(describing: error) : error.localizedDescription
    }

    @objc private func selectLanguage(_ sender: NSMenuItem) {
        guard let raw = sender.representedObject as? String,
              let option = Language(rawValue: raw) else { return }
        UserDefaults.standard.set(option.rawValue, forKey: languageDefaultsKey)
        requestMenuRebuild()
    }

    /// Quitting the menu bar app is quitting the whole stack: stop the
    /// CodexMux proxy (which also restores the managed Codex configuration),
    /// stop the CPA service, then terminate. A failed proxy stop is shown
    /// before anything else happens so the user can keep the app running.
    @objc private func confirmQuit() {
        guard !quitInProgress else { return }
        let l10n = self.l10n
        let choice = runAlert(
            title: l10n.quitDialogTitle,
            body: l10n.quitDialogBody,
            buttons: [l10n.quitDialogConfirm, l10n.quitDialogCancel]
        )
        guard choice == .alertFirstButtonReturn else { return }

        quitInProgress = true
        requestMenuRebuild()
        showProgress(l10n.quitting)
        runCodexMuxInBackground(["uninstall"]) { [weak self] result in
            guard let self else { return }
            guard !result.succeeded else {
                self.stopCPAAndTerminate()
                return
            }
            self.hideProgress()
            self.presentWhenMenuClosed {
                let l10n = self.l10n
                let choice = self.runAlert(
                    title: l10n.quitFailedTitle,
                    body: result.failureDetail,
                    buttons: [l10n.quitAnyway, l10n.quitDialogCancel],
                    style: .warning
                )
                if choice == .alertFirstButtonReturn {
                    self.showProgress(l10n.quitting)
                    self.stopCPAAndTerminate()
                } else {
                    self.quitInProgress = false
                    self.requestMenuRebuild()
                    self.refreshStatus()
                }
            }
        }
    }

    /// Stop the local CPA without touching its startup preference, so the
    /// next launch honors the user's last explicit choice, then quit.
    private func stopCPAAndTerminate() {
        runCodexMuxInBackground(["cpa", "stop", "--no-preference"]) { _ in
            NSApp.terminate(nil)
        }
    }

    @objc private func restartProxy() {
        // Restarting CodexMux means reinstalling its LaunchAgent, which also
        // re-enables the managed Codex configuration.
        runCodexMuxDetached(["install"]) { [weak self] in
            self?.reportFailure($0, \.restartFailed)
        }
    }

    @objc private func stopProxy() {
        runCodexMuxDetached(["uninstall"]) { [weak self] in
            self?.reportFailure($0, \.stopFailed)
        }
    }

    @objc private func startCPA() {
        runCodexMuxDetached(["cpa", "start"]) { [weak self] result in
            guard let self else { return }
            guard result.succeeded else {
                self.reportFailure(result, \.startCPAFailed)
                return
            }
            self.refreshCPAModelLists()
        }
    }

    @objc private func stopCPA() {
        runCodexMuxDetached(["cpa", "stop"]) { [weak self] in
            self?.reportFailure($0, \.stopCPAFailed)
        }
    }

    @objc private func installCPA() {
        guard !cpaInstalling else { return }
        cpaInstalling = true
        runCodexMuxDetached(["cpa", "install"]) { [weak self] result in
            guard let self else { return }
            self.cpaInstalling = false
            guard result.succeeded else {
                self.reportFailure(result, \.installCPAFailed)
                return
            }
            // Installation also starts CPA and enables autostart. Hand off
            // to its Web UI for provider credentials.
            self.refreshCPAModelLists()
            self.openCpaManagement()
        }
    }

    @objc private func toggleCPAAutostart(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["cpa", "autostart-set", String(enabled)]) { [weak self] in
            self?.reportFailure($0, \.autostartSetFailed)
        }
    }

    @objc private func toggleAdvertiseUltra(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["catalog", "ultra-set", String(enabled)]) { [weak self] result in
            guard let self else { return }
            guard result.succeeded else {
                self.reportFailure(result, \.advertiseUltraFailed)
                return
            }
            // The proxy reads catalog settings when it starts.
            self.restartProxy()
        }
    }

    @objc private func toggleUnifyCompHash(_ sender: NSMenuItem) {
        let enabled = sender.state != .on
        runCodexMuxDetached(["catalog", "comp-hash-set", String(enabled)]) { [weak self] result in
            guard let self else { return }
            guard result.succeeded else {
                self.reportFailure(result, \.unifyCompHashFailed)
                return
            }
            self.restartProxy()
        }
    }

    @objc private func checkCpaUpdate() {
        guard !cpaCheckingUpdate else { return }
        cpaCheckingUpdate = true
        runCodexMuxInBackground(["cpa", "update-check"]) { [weak self] result in
            guard let self else { return }
            self.cpaCheckingUpdate = false
            let prefix = "latest: "
            let latest = result.stdoutLines
                .first { $0.hasPrefix(prefix) }
                .map { String($0.dropFirst(prefix.count)) }
            guard result.succeeded, let latest, !latest.isEmpty else {
                self.showAlert(self.l10n.cpaUpdateCheckFailed, detail: result.failureDetail)
                return
            }
            self.cpaLatestVersion = latest
            self.cpaUpdateAvailable = result.stdoutLines.contains("update available: true")
        }
    }

    @objc private func updateCpa() {
        guard !cpaUpdating, let target = cpaLatestVersion, cpaUpdateAvailable else { return }
        let l10n = self.l10n
        let choice = runAlert(
            title: l10n.cpaUpdateDialogTitle,
            body: l10n.cpaUpdateDialogBody(from: state?.cpa.version ?? "unknown", to: target),
            buttons: [l10n.cpaUpdateTo(target), l10n.quitDialogCancel]
        )
        guard choice == .alertFirstButtonReturn else { return }

        cpaUpdating = true
        // Install exactly the version the user confirmed, even if a newer
        // release was published after the check.
        runCodexMuxDetached(["cpa", "update", "--version", target]) { [weak self] result in
            guard let self else { return }
            self.cpaUpdating = false
            guard result.succeeded else {
                self.reportFailure(result, \.cpaUpdateFailed)
                return
            }
            // The installed version changed, so the last check is stale.
            self.cpaLatestVersion = nil
            self.cpaUpdateAvailable = false
            self.refreshCPAModelLists()
        }
    }

    @objc private func rollbackCpa() {
        guard !cpaUpdating, state?.cpa.rollbackAvailable == true else { return }
        let l10n = self.l10n
        let choice = runAlert(
            title: l10n.cpaRollbackDialogTitle,
            body: l10n.cpaRollbackDialogBody,
            buttons: [l10n.cpaRollback, l10n.quitDialogCancel]
        )
        guard choice == .alertFirstButtonReturn else { return }

        cpaUpdating = true
        runCodexMuxDetached(["cpa", "rollback"]) { [weak self] result in
            guard let self else { return }
            self.cpaUpdating = false
            guard result.succeeded else {
                self.reportFailure(result, \.cpaRollbackFailed)
                return
            }
            self.cpaLatestVersion = nil
            self.cpaUpdateAvailable = false
            self.refreshCPAModelLists()
        }
    }

    @objc private func selectReviewModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "review-set", slug]) { [weak self] in
            self?.reportFailure($0, \.reviewSetFailed)
        }
    }

    @objc private func selectImageModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "image-set", slug]) { [weak self] in
            self?.reportFailure($0, \.imageSetFailed)
        }
    }

    @objc private func selectSearchBackendDefault() {
        setSearchBackend("default")
    }

    @objc private func selectSearchBackendDisabled() {
        setSearchBackend("off")
    }

    @objc private func selectSearchBackendModel(_ sender: NSMenuItem) {
        guard let slug = sender.representedObject as? String else { return }
        setSearchBackend(slug)
    }

    private func setSearchBackend(_ value: String) {
        runCodexMuxDetached(["cpa", "search-set", value]) { [weak self] in
            self?.reportFailure($0, \.searchSetFailed)
        }
    }

    @objc private func toggleVerifiedOnly(_ sender: NSMenuItem) {
        searchShowVerifiedOnly = sender.state != .on
        UserDefaults.standard.set(searchShowVerifiedOnly, forKey: "searchShowVerifiedOnly")
    }

    @objc private func detectSearchBackends() {
        guard !searchDetectRunning else { return }
        searchDetectRunning = true
        runCodexMuxDetached(
            ["cpa", "search-detect"],
            progressText: l10n.searchDetectRunning
        ) { [weak self] result in
            guard let self else { return }
            self.searchDetectRunning = false
            self.reportFailure(result, \.searchDetectFailed)
        }
    }

    @objc private func switchProfile(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        runCodexMuxDetached(["cpa", "profile-switch", name]) { [weak self] result in
            guard let self else { return }
            guard result.succeeded else {
                self.reportFailure(result) { $0.profileSwitchFailed(name) }
                return
            }
            // The cached lists describe the previous endpoint.
            self.cpaModels = []
            self.imageModels = []
            self.refreshCPAModelLists()
        }
    }

    @objc private func openLogs() {
        NSWorkspace.shared.open(URL(fileURLWithPath: codexmuxHome + "/logs"))
    }

    @objc private func openCpaManagement() {
        // For a loopback CPA the URL carries the management key, so this
        // command's output never goes into an alert.
        runCodexMuxInBackground(["cpa", "management-url", "--connect"]) { [weak self] result in
            let url = result.succeeded
                ? result.stdoutLines
                    .compactMap { URL(string: $0) }
                    .first { $0.scheme == "http" || $0.scheme == "https" }
                : nil
            guard let url, NSWorkspace.shared.open(url) else {
                self?.showAlert(self?.l10n.openCpaManagementFailed ?? "")
                return
            }
        }
    }

    @objc private func copyCpaManagementKey() {
        // This command prints the key itself; its output never goes into an alert.
        runCodexMuxInBackground(["cpa", "management-key"]) { [weak self] result in
            guard let self else { return }
            let prefix = "management-key: "
            let key = result.succeeded
                ? result.stdoutLines
                    .first { $0.hasPrefix(prefix) }
                    .map { String($0.dropFirst(prefix.count)) }
                : nil
            guard let key, !key.isEmpty else {
                self.showAlert(self.l10n.copyCpaManagementKeyFailed)
                return
            }
            NSPasteboard.general.clearContents()
            guard NSPasteboard.general.setString(key, forType: .string) else {
                self.showAlert(self.l10n.copyCpaManagementKeyFailed)
                return
            }
        }
    }

    // MARK: - Process helpers

    private var codexMuxEnvironment: [String: String] {
        let inherited = ProcessInfo.processInfo.environment
        var environment = [
            "CODEXMUX_HOME": codexmuxHome,
            "PATH": "\(NSHomeDirectory())/.local/bin:/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin",
            // Keep terminal color codes out of output shown in alerts.
            "NO_COLOR": "1",
        ]
        for name in ["CODEX_CONFIG", "CODEX_HOME", "HOME", "TMPDIR", "LANG", "LC_ALL"] {
            if let value = inherited[name], !value.isEmpty {
                environment[name] = value
            }
        }
        return environment
    }

    /// Run codexmux synchronously (background queues only).
    private func runCodexMux(_ arguments: [String], timeout: TimeInterval? = nil) -> CommandResult {
        CommandRunner(executableURL: codexmuxURL, environment: codexMuxEnvironment)
            .run(arguments, timeout: timeout)
    }

    /// Run codexmux on a background queue; `completion` runs on the main queue.
    private func runCodexMuxInBackground(
        _ arguments: [String],
        timeout: TimeInterval? = nil,
        completion: @escaping (CommandResult) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            let result = self.runCodexMux(arguments, timeout: timeout)
            DispatchQueue.main.async { completion(result) }
        }
    }

    /// Run a user-initiated CLI action, then refresh the local state. Long
    /// actions pass `progressText` so a small floating window keeps the work
    /// visible after the status menu collapses.
    private func runCodexMuxDetached(
        _ arguments: [String],
        progressText: String? = nil,
        completion: @escaping (CommandResult) -> Void
    ) {
        if let progressText {
            showProgress(progressText)
        }
        runCodexMuxInBackground(arguments) { [weak self] result in
            if progressText != nil {
                self?.hideProgress()
            }
            completion(result)
            self?.refreshStatus()
        }
    }

    // MARK: - Alerts and progress

    /// Show `message(l10n)` with the end of the command's output if `result` failed.
    private func reportFailure(_ result: CommandResult, _ message: (L10n) -> String) {
        guard !result.succeeded else { return }
        showAlert(message(l10n), detail: result.failureDetail)
    }

    private func showAlert(_ message: String, detail: String = "") {
        presentWhenMenuClosed { [weak self] in
            guard let self else { return }
            self.runAlert(title: message, body: detail, buttons: [self.l10n.alertOK])
        }
    }

    /// Run `presentation` (typically a modal alert) on the next main-queue
    /// turn, or once the status menu closes if it is open then.
    private func presentWhenMenuClosed(_ presentation: @escaping () -> Void) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if self.menuIsOpen {
                self.deferredPresentations.append(presentation)
            } else {
                presentation()
            }
        }
    }

    @discardableResult
    private func runAlert(
        title: String,
        body: String = "",
        buttons: [String],
        style: NSAlert.Style = .informational
    ) -> NSApplication.ModalResponse {
        let alert = NSAlert()
        alert.alertStyle = style
        alert.messageText = title
        alert.informativeText = body
        for button in buttons {
            alert.addButton(withTitle: button)
        }
        // An accessory app is not active by default; bring the alert forward.
        NSApp.activate(ignoringOtherApps: true)
        return alert.runModal()
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
}
