import Cocoa

final class DirectEndpointWindowController: NSWindowController, NSTableViewDataSource, NSTableViewDelegate {
    private let baseURLField = NSTextField()
    private let tokenField = NSSecureTextField()
    private let manualField = NSTextField()
    private let discoverButton = NSButton()
    private let statusLabel = NSTextField(labelWithString: "")
    private let tableView = NSTableView()
    private let selectAllButton = NSButton()
    private let clearButton = NSButton()
    private let cancelButton = NSButton()
    private let saveButton = NSButton()

    private var models: [String] = []
    private var selectedModels: Set<String> = []
    private var l10n = L10n.forLanguage(.systemPreferred)
    private var onDiscover: ((String, String) -> Void)?
    private var onSave: ((String, String, [String], @escaping (Bool) -> Void) -> Void)?

    init() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 580, height: 480),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
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
        l10n: L10n,
        onDiscover: @escaping (String, String) -> Void,
        onSave: @escaping (String, String, [String], @escaping (Bool) -> Void) -> Void
    ) {
        self.l10n = l10n
        self.onDiscover = onDiscover
        self.onSave = onSave
        window?.title = l10n.directDialogTitle
        baseURLField.stringValue = ""
        tokenField.stringValue = ""
        manualField.stringValue = ""
        baseURLField.placeholderString = "https://example.com/v1"
        tokenField.placeholderString = l10n.directTokenPlaceholder
        manualField.placeholderString = l10n.directManualPlaceholder
        discoverButton.title = l10n.directDiscover
        discoverButton.isEnabled = true
        selectAllButton.title = l10n.directSelectAll
        clearButton.title = l10n.directClearAll
        cancelButton.title = l10n.quitDialogCancel
        saveButton.title = l10n.directSave
        saveButton.isEnabled = true
        models = []
        selectedModels = []
        statusLabel.stringValue = ""
        statusLabel.textColor = .secondaryLabelColor
        tableView.reloadData()
    }

    func present() {
        showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func setDiscovering(_ discovering: Bool) {
        discoverButton.isEnabled = !discovering
        discoverButton.title = discovering ? l10n.directDiscovering : l10n.directDiscover
    }

    func setModels(_ models: [String]) {
        self.models = models
        selectedModels = Set(models)
        tableView.reloadData()
        if models.isEmpty {
            statusLabel.stringValue = l10n.directNoModels
        } else {
            statusLabel.stringValue = String(format: l10n.directModelsFound, models.count)
        }
        statusLabel.textColor = .secondaryLabelColor
    }

    func setError(_ message: String) {
        statusLabel.stringValue = message
        statusLabel.textColor = .systemRed
    }

    func finishSave(_ success: Bool, message: String?) {
        saveButton.isEnabled = true
        if !success, let message {
            setError(message)
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        models.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard row < models.count, let tableColumn else { return nil }
        let cell = NSTableCellView()
        cell.identifier = tableColumn.identifier
        if tableColumn.identifier.rawValue == "check" {
            let button = NSButton(
                checkboxWithTitle: "",
                target: self,
                action: #selector(toggleModel(_:))
            )
            button.tag = row
            button.state = selectedModels.contains(models[row]) ? .on : .off
            button.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(button)
            NSLayoutConstraint.activate([
                button.centerXAnchor.constraint(equalTo: cell.centerXAnchor),
                button.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        } else {
            let label = NSTextField(labelWithString: models[row])
            label.lineBreakMode = .byTruncatingMiddle
            label.translatesAutoresizingMaskIntoConstraints = false
            cell.addSubview(label)
            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 4),
                label.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -4),
                label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        }
        return cell
    }

    private func buildContent() {
        guard let content = window?.contentView else { return }
        let root = NSStackView()
        root.orientation = .vertical
        root.alignment = .leading
        root.spacing = 10
        root.translatesAutoresizingMaskIntoConstraints = false
        content.addSubview(root)

        root.addArrangedSubview(fieldRow(l10n.directDialogBaseURL, baseURLField))
        root.addArrangedSubview(fieldRow(l10n.directDialogToken, tokenField))

        let actionRow = NSStackView()
        actionRow.orientation = .horizontal
        actionRow.spacing = 10
        actionRow.alignment = .centerY
        discoverButton.target = self
        discoverButton.action = #selector(discover)
        actionRow.addArrangedSubview(discoverButton)
        statusLabel.lineBreakMode = .byWordWrapping
        statusLabel.maximumNumberOfLines = 2
        statusLabel.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        actionRow.addArrangedSubview(statusLabel)
        root.addArrangedSubview(actionRow)

        let checkColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("check"))
        checkColumn.width = 42
        let modelColumn = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("model"))
        modelColumn.width = 478
        tableView.addTableColumn(checkColumn)
        tableView.addTableColumn(modelColumn)
        tableView.headerView = nil
        tableView.rowHeight = 30
        tableView.dataSource = self
        tableView.delegate = self
        let scroll = NSScrollView()
        scroll.documentView = tableView
        scroll.hasVerticalScroller = true
        scroll.borderType = .bezelBorder
        scroll.translatesAutoresizingMaskIntoConstraints = false
        scroll.heightAnchor.constraint(equalToConstant: 180).isActive = true
        scroll.widthAnchor.constraint(equalToConstant: 520).isActive = true
        root.addArrangedSubview(scroll)

        let selectionRow = NSStackView()
        selectionRow.orientation = .horizontal
        selectionRow.spacing = 8
        selectionRow.alignment = .centerY
        selectAllButton.target = self
        selectAllButton.action = #selector(selectAllModels(_:))
        clearButton.target = self
        clearButton.action = #selector(clearAllModels(_:))
        selectionRow.addArrangedSubview(selectAllButton)
        selectionRow.addArrangedSubview(clearButton)
        root.addArrangedSubview(selectionRow)

        root.addArrangedSubview(fieldRow(l10n.directManualModels, manualField))

        let buttonRow = NSStackView()
        buttonRow.orientation = .horizontal
        buttonRow.spacing = 10
        buttonRow.alignment = .centerY
        let spacer = NSView()
        spacer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        buttonRow.addArrangedSubview(spacer)
        cancelButton.target = self
        cancelButton.action = #selector(cancel)
        cancelButton.keyEquivalent = "\u{1b}"
        saveButton.target = self
        saveButton.action = #selector(save)
        saveButton.keyEquivalent = "\r"
        buttonRow.addArrangedSubview(cancelButton)
        buttonRow.addArrangedSubview(saveButton)
        root.addArrangedSubview(buttonRow)

        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 20),
            root.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -20),
            root.topAnchor.constraint(equalTo: content.topAnchor, constant: 18),
            root.bottomAnchor.constraint(lessThanOrEqualTo: content.bottomAnchor, constant: -18),
        ])
    }

    private func fieldRow(_ label: String, _ field: NSTextField) -> NSView {
        let row = NSStackView()
        row.orientation = .horizontal
        row.spacing = 10
        row.alignment = .centerY
        row.translatesAutoresizingMaskIntoConstraints = false
        let text = NSTextField(labelWithString: label)
        text.widthAnchor.constraint(equalToConstant: 130).isActive = true
        field.translatesAutoresizingMaskIntoConstraints = false
        field.widthAnchor.constraint(equalToConstant: 380).isActive = true
        row.addArrangedSubview(text)
        row.addArrangedSubview(field)
        return row
    }

    @objc private func discover() {
        let baseURL = baseURLField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let token = tokenField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !baseURL.isEmpty, !token.isEmpty else {
            setError(l10n.directMissingFields)
            return
        }
        setDiscovering(true)
        onDiscover?(baseURL, token)
    }

    @objc private func save() {
        let baseURL = baseURLField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let token = tokenField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        var combined = Set(models.filter { selectedModels.contains($0) })
        for rawModel in manualField.stringValue.split(separator: ",") {
            let model = rawModel.trimmingCharacters(in: .whitespacesAndNewlines)
            if !model.isEmpty {
                combined.insert(model)
            }
        }
        let models = combined.sorted()
        guard !baseURL.isEmpty, !token.isEmpty, !models.isEmpty else {
            setError(models.isEmpty ? l10n.directMissingModels : l10n.directMissingFields)
            return
        }
        saveButton.isEnabled = false
        onSave?(baseURL, token, models) { [weak self] success in
            guard let self else { return }
            if success {
                self.close()
            } else {
                self.finishSave(false, message: self.l10n.directSetFailed)
            }
        }
    }

    @objc private func selectAllModels(_ sender: Any?) {
        selectedModels = Set(models)
        tableView.reloadData()
    }

    @objc private func clearAllModels(_ sender: Any?) {
        selectedModels = []
        tableView.reloadData()
    }

    @objc private func toggleModel(_ sender: NSButton) {
        guard sender.tag < models.count else { return }
        let model = models[sender.tag]
        if sender.state == .on {
            selectedModels.insert(model)
        } else {
            selectedModels.remove(model)
        }
    }

    @objc private func cancel() {
        close()
    }
}
