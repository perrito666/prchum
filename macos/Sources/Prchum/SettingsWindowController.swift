import AppKit
import PrchumKit

/// Settings: appearance, theme, and the discovery filters — written
/// through to config.json (the file stays hand-editable; unknown keys
/// survive every save).
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate,
    NSTableViewDataSource, NSTableViewDelegate
{
    private let appearancePicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private let themePicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private let authorField = NSTextField(string: "")
    private let defaultFilterField = NSTextField(string: "")
    private let filtersTable = NSTableView()
    /// (name, filter), sorted by name.
    private var filters: [(String, String)] = []
    private let editorField = NSTextField(string: "")
    private let clonesTable = NSTableView()
    /// (owner/repo, path), sorted by repository.
    private var clones: [(String, String)] = []
    private let onChange: () -> Void

    var onClose: (() -> Void)?

    init(onChange: @escaping () -> Void) {
        self.onChange = onChange
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 470, height: 720),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false)
        window.title = "Settings"
        super.init(window: window)
        window.delegate = self

        let config = CoreConfig()

        appearancePicker.addItems(withTitles: ["System", "Light", "Dark"])
        appearancePicker.target = self
        appearancePicker.action = #selector(appearanceChanged(_:))
        appearancePicker.selectItem(at: Int(config.appearance.rawValue))

        themePicker.addItems(withTitles: Self.themeNames())
        themePicker.target = self
        themePicker.action = #selector(themeChanged(_:))
        let currentTheme = config.theme.isEmpty ? "default" : config.theme
        themePicker.selectItem(withTitle: currentTheme)
        if themePicker.selectedItem == nil {
            themePicker.selectItem(at: 0)
        }

        authorField.placeholderString = NSUserName()
        authorField.stringValue = config.author == NSUserName() ? "" : config.author
        authorField.target = self
        authorField.action = #selector(authorChanged(_:))

        defaultFilterField.placeholderString = "engine default (review-requested:@me)"
        defaultFilterField.stringValue = config.listFilter
        defaultFilterField.target = self
        defaultFilterField.action = #selector(defaultFilterChanged(_:))

        for (identifier, title, width) in [("name", "Name", 110), ("filter", "Filter", 250)] {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = title
            column.width = CGFloat(width)
            filtersTable.addTableColumn(column)
        }
        filtersTable.dataSource = self
        filtersTable.delegate = self
        filtersTable.usesAlternatingRowBackgroundColors = true
        // A scroll view leaves its document view at whatever size it
        // arrives with, and a table built in code arrives at zero — which
        // draws nothing at all, headers included.
        filtersTable.frame = NSRect(x: 0, y: 0, width: 300, height: 130)
        filtersTable.autoresizingMask = [.width]
        reloadFilters()

        editorField.placeholderString = CoreEditorDefaults.template
        editorField.stringValue = config.editorCommand
        editorField.target = self
        editorField.action = #selector(editorChanged(_:))

        for (identifier, title, width) in [("repo", "Repository", 150), ("path", "Clone", 210)] {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(identifier))
            column.title = title
            column.width = CGFloat(width)
            clonesTable.addTableColumn(column)
        }
        clonesTable.dataSource = self
        clonesTable.delegate = self
        clonesTable.usesAlternatingRowBackgroundColors = true
        clonesTable.frame = NSRect(x: 0, y: 0, width: 300, height: 110)
        clonesTable.autoresizingMask = [.width]
        reloadClones()

        let filtersScroll = NSScrollView()
        filtersScroll.hasVerticalScroller = true
        filtersScroll.documentView = filtersTable
        filtersScroll.translatesAutoresizingMaskIntoConstraints = false
        filtersScroll.heightAnchor.constraint(equalToConstant: 130).isActive = true

        let clonesScroll = NSScrollView()
        clonesScroll.hasVerticalScroller = true
        clonesScroll.documentView = clonesTable
        clonesScroll.translatesAutoresizingMaskIntoConstraints = false
        clonesScroll.heightAnchor.constraint(equalToConstant: 110).isActive = true

        let cloneButtons = NSStackView()
        cloneButtons.orientation = .horizontal
        cloneButtons.spacing = 8
        cloneButtons.addArrangedSubview(
            NSButton(title: "Add…", target: self, action: #selector(addClone(_:))))
        cloneButtons.addArrangedSubview(
            NSButton(title: "Remove", target: self, action: #selector(removeClone(_:))))

        let filterButtons = NSStackView()
        filterButtons.orientation = .horizontal
        filterButtons.spacing = 8
        filterButtons.addArrangedSubview(
            NSButton(title: "Add…", target: self, action: #selector(addFilter(_:))))
        filterButtons.addArrangedSubview(
            NSButton(title: "Remove", target: self, action: #selector(removeFilter(_:))))
        filterButtons.addArrangedSubview(
            NSButton(title: "Edit…", target: self, action: #selector(editFilter(_:))))

        let grid = NSGridView(views: [
            [NSTextField(labelWithString: "Appearance:"), appearancePicker],
            [NSTextField(labelWithString: "Theme:"), themePicker],
            [
                NSTextField(labelWithString: ""),
                {
                    let hint = NSTextField(
                        wrappingLabelWithString:
                            "Themes are JSON files in the themes folder next to config.json.")
                    hint.textColor = .secondaryLabelColor
                    hint.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                    return hint
                }(),
            ],
            [NSTextField(labelWithString: "Author:"), authorField],
            [
                NSTextField(labelWithString: ""),
                {
                    let hint = NSTextField(
                        wrappingLabelWithString:
                            "Drafts are attributed to this name — your forge handle, "
                            + "which is rarely the account name.")
                    hint.textColor = .secondaryLabelColor
                    hint.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                    return hint
                }(),
            ],
            [NSTextField(labelWithString: "Default filter:"), defaultFilterField],
            [NSTextField(labelWithString: "Named filters:"), filtersScroll],
            [NSTextField(labelWithString: ""), filterButtons],
            [
                NSTextField(labelWithString: ""),
                {
                    let hint = NSTextField(
                        wrappingLabelWithString:
                            "Named filters appear in the review queue's picker; "
                            + "the default runs when none is chosen.")
                    hint.textColor = .secondaryLabelColor
                    hint.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                    return hint
                }(),
            ],
            [NSTextField(labelWithString: "Editor:"), editorField],
            [NSTextField(labelWithString: "Local clones:"), clonesScroll],
            [NSTextField(labelWithString: ""), cloneButtons],
            [
                NSTextField(labelWithString: ""),
                {
                    let hint = NSTextField(
                        wrappingLabelWithString:
                            "Edit File Locally checks the branch out of the clone you "
                            + "point at here, then opens the file in the editor — a URL "
                            + "or a command, with {path}, {line}, and {dir}.")
                    hint.textColor = .secondaryLabelColor
                    hint.font = .systemFont(ofSize: NSFont.smallSystemFontSize)
                    return hint
                }(),
            ],
        ])
        grid.rowSpacing = 10
        grid.column(at: 0).xPlacement = .trailing
        grid.column(at: 1).width = 300
        let stack = NSStackView(views: [grid])
        stack.edgeInsets = NSEdgeInsets(top: 16, left: 16, bottom: 16, right: 16)
        window.contentView = stack
        window.center()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("not used")
    }

    func windowWillClose(_ notification: Notification) {
        onClose?()
    }

    /// Built-ins plus every themes/*.json next to the config file.
    private static func themeNames() -> [String] {
        var names = CoreSyntax.builtinThemeNames
        let themesDir = (CoreConfig.defaultPath as NSString)
            .deletingLastPathComponent + "/themes"
        if let files = try? FileManager.default.contentsOfDirectory(atPath: themesDir) {
            for file in files.sorted() where file.hasSuffix(".json") {
                let name = String(file.dropLast(5))
                if !names.contains(name) {
                    names.append(name)
                }
            }
        }
        return names
    }

    @objc private func appearanceChanged(_ sender: Any?) {
        let value = ["system", "light", "dark"][appearancePicker.indexOfSelectedItem]
        CoreConfig.setString("appearance", value)
        onChange()
    }

    @objc private func themeChanged(_ sender: Any?) {
        let name = themePicker.titleOfSelectedItem ?? "default"
        CoreConfig.setString("theme", name)
        onChange()
    }

    // MARK: - Filters

    private func reloadFilters() {
        filters = CoreConfig().listFilters.sorted { $0.key < $1.key }
            .map { ($0.key, $0.value) }
        filtersTable.reloadData()
    }

    @objc private func authorChanged(_ sender: Any?) {
        CoreConfig.setString(
            "author", authorField.stringValue.trimmingCharacters(in: .whitespaces))
    }

    @objc private func defaultFilterChanged(_ sender: Any?) {
        CoreConfig.setString(
            "list_filter",
            defaultFilterField.stringValue.trimmingCharacters(in: .whitespaces))
    }

    @objc private func addFilter(_ sender: Any?) {
        promptForFilter(name: "", filter: "")
    }

    @objc private func editFilter(_ sender: Any?) {
        let row = filtersTable.selectedRow
        guard filters.indices.contains(row) else { return }
        promptForFilter(name: filters[row].0, filter: filters[row].1)
    }

    @objc private func removeFilter(_ sender: Any?) {
        let row = filtersTable.selectedRow
        guard filters.indices.contains(row) else { return }
        CoreConfig.setMapEntry("list_filters", filters[row].0, "")
        reloadFilters()
    }

    private func promptForFilter(name: String, filter: String) {
        guard let window else { return }
        let alert = NSAlert()
        alert.messageText = name.isEmpty ? "New filter" : "Edit \(name)"
        let nameField = NSTextField(frame: NSRect(x: 0, y: 34, width: 380, height: 24))
        nameField.placeholderString = "name (e.g. bugs)"
        nameField.stringValue = name
        nameField.isEditable = name.isEmpty
        let filterField = NSTextField(frame: NSRect(x: 0, y: 0, width: 380, height: 24))
        filterField.placeholderString = "is:open label:bug review-requested:@me"
        filterField.stringValue = filter
        let holder = NSView(frame: NSRect(x: 0, y: 0, width: 380, height: 62))
        holder.addSubview(nameField)
        holder.addSubview(filterField)
        alert.accessoryView = holder
        alert.window.initialFirstResponder = name.isEmpty ? nameField : filterField
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertFirstButtonReturn else { return }
            let newName = nameField.stringValue.trimmingCharacters(in: .whitespaces)
            let newFilter = filterField.stringValue.trimmingCharacters(in: .whitespaces)
            guard !newName.isEmpty, !newFilter.isEmpty else { return }
            CoreConfig.setMapEntry("list_filters", newName, newFilter)
            self?.reloadFilters()
        }
    }

    // MARK: - Clones

    private func reloadClones() {
        clones = CoreConfig().clones.sorted { $0.key < $1.key }.map { ($0.key, $0.value) }
        clonesTable.reloadData()
    }

    @objc private func editorChanged(_ sender: Any?) {
        CoreConfig.setString(
            "editor_command",
            editorField.stringValue.trimmingCharacters(in: .whitespaces))
    }

    /// A clone is a repository plus a directory: ask for the directory
    /// first, since `git remote` there usually names the repository too.
    @objc private func addClone(_ sender: Any?) {
        guard let window else { return }
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.message = "Choose a local clone"
        guard panel.runModal() == .OK, let url = panel.url else { return }

        let alert = NSAlert()
        alert.messageText = "Which repository is this?"
        alert.informativeText =
            "As the forge names it — owner/repo, or group/subgroup/repo."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 380, height: 24))
        field.placeholderString = "owner/repo"
        field.stringValue = Self.slugFromOrigin(at: url.path) ?? ""
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        alert.beginSheetModal(for: window) { [weak self] response in
            guard response == .alertFirstButtonReturn else { return }
            let slug = field.stringValue.trimmingCharacters(in: .whitespaces)
            guard !slug.isEmpty else { return }
            CoreConfig.setMapEntry("clones", slug, url.path)
            self?.reloadClones()
        }
    }

    @objc private func removeClone(_ sender: Any?) {
        let row = clonesTable.selectedRow
        guard clones.indices.contains(row) else { return }
        CoreConfig.setMapEntry("clones", clones[row].0, "")
        reloadClones()
    }

    /// `owner/repo` read out of the clone's origin remote, when it has one.
    private static func slugFromOrigin(at path: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["git", "-C", path, "config", "--get", "remote.origin.url"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        var remote = String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !remote.isEmpty else { return nil }
        if remote.hasSuffix(".git") {
            remote = String(remote.dropLast(4))
        }
        // git@host:owner/repo and https://host/owner/repo both end in the
        // slug; take everything after the host.
        if let range = remote.range(of: "://") {
            let rest = remote[range.upperBound...]
            return rest.split(separator: "/").dropFirst().joined(separator: "/")
        }
        if let colon = remote.firstIndex(of: ":") {
            return String(remote[remote.index(after: colon)...])
        }
        return nil
    }

    // MARK: table

    func numberOfRows(in tableView: NSTableView) -> Int {
        tableView === clonesTable ? clones.count : filters.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let text: String
        if tableView === clonesTable {
            let (slug, path) = clones[row]
            text = tableColumn?.identifier.rawValue == "repo" ? slug : path
        } else {
            let (name, filter) = filters[row]
            text = tableColumn?.identifier.rawValue == "name" ? name : filter
        }
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }
}

/// The editor default, shown as the field's placeholder.
enum CoreEditorDefaults {
    static let template = "textchum://open?path={path}&line={line}"
}
