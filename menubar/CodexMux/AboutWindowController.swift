import Cocoa

final class AboutWindowController: NSWindowController {
    private let repositoryURL: URL
    private let versionLabel = NSTextField(labelWithString: "")
    private let cliVersionLabel = NSTextField(labelWithString: "")
    private let repositoryButton = NSButton()
    private let statusLabel = NSTextField(labelWithString: "")
    private let updateButton = NSButton()
    private var onCheckForUpdates: (() -> Void)?
    private var l10n = L10n.forLanguage(.systemPreferred)

    init(repositoryURL: URL) {
        self.repositoryURL = repositoryURL
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 330),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "CodexMux"
        window.center()
        window.isReleasedWhenClosed = false
        super.init(window: window)
        buildContent()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(
        appVersion: String,
        cliVersion: String,
        l10n: L10n,
        isChecking: Bool,
        onCheckForUpdates: @escaping () -> Void
    ) {
        self.l10n = l10n
        self.onCheckForUpdates = onCheckForUpdates
        versionLabel.stringValue = l10n.isChinese
            ? "App 版本：\(appVersion)"
            : "App version: \(appVersion)"
        cliVersionLabel.stringValue = l10n.isChinese
            ? "内置 CLI：\(cliVersion)"
            : "Bundled CLI: \(cliVersion)"
        repositoryButton.title = repositoryURL.absoluteString
        repositoryButton.toolTip = l10n.openRepository
        updateButton.title = isChecking ? l10n.checkingAppUpdates : l10n.checkAppUpdates
        updateButton.isEnabled = !isChecking
        if !isChecking {
            statusLabel.stringValue = ""
        }
        window?.title = l10n.about.replacingOccurrences(of: "…", with: "")
    }

    func setUpdateState(message: String, isChecking: Bool) {
        statusLabel.stringValue = message
        updateButton.isEnabled = !isChecking
        updateButton.title = isChecking
            ? l10n.checkingAppUpdates
            : l10n.checkAppUpdates
    }

    private func buildContent() {
        guard let contentView = window?.contentView else { return }

        let icon = NSImageView(image: NSApp.applicationIconImage)
        icon.imageScaling = .scaleProportionallyUpOrDown
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.widthAnchor.constraint(equalToConstant: 92).isActive = true
        icon.heightAnchor.constraint(equalToConstant: 92).isActive = true

        let title = NSTextField(labelWithString: "CodexMux")
        title.font = .systemFont(ofSize: 25, weight: .semibold)
        title.alignment = .center

        repositoryButton.bezelStyle = .inline
        repositoryButton.isBordered = false
        repositoryButton.contentTintColor = .linkColor
        repositoryButton.target = self
        repositoryButton.action = #selector(openRepository)

        statusLabel.textColor = .secondaryLabelColor
        statusLabel.alignment = .center
        statusLabel.maximumNumberOfLines = 2

        updateButton.bezelStyle = .rounded
        updateButton.target = self
        updateButton.action = #selector(checkForUpdates)

        let stack = NSStackView(views: [
            icon,
            title,
            versionLabel,
            cliVersionLabel,
            repositoryButton,
            statusLabel,
            updateButton,
        ])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 9
        stack.setCustomSpacing(14, after: icon)
        stack.setCustomSpacing(16, after: repositoryButton)
        stack.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(greaterThanOrEqualTo: contentView.leadingAnchor, constant: 28),
            stack.trailingAnchor.constraint(lessThanOrEqualTo: contentView.trailingAnchor, constant: -28),
            stack.centerXAnchor.constraint(equalTo: contentView.centerXAnchor),
            stack.centerYAnchor.constraint(equalTo: contentView.centerYAnchor),
            statusLabel.widthAnchor.constraint(lessThanOrEqualToConstant: 390),
        ])
    }

    @objc private func openRepository() {
        NSWorkspace.shared.open(repositoryURL)
    }

    @objc private func checkForUpdates() {
        onCheckForUpdates?()
    }
}
