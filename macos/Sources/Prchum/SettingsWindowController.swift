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
    private let defaultFilterField = NSTextField(string: "")
    private let filtersTable = NSTableView()
    /// (name, filter), sorted by name.
    private var filters: [(String, String)] = []
    private let onChange: () -> Void

    var onClose: (() -> Void)?

    init(onChange: @escaping () -> Void) {
        self.onChange = onChange
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 460, height: 420),
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
        reloadFilters()

        let filtersScroll = NSScrollView()
        filtersScroll.hasVerticalScroller = true
        filtersScroll.documentView = filtersTable
        filtersScroll.translatesAutoresizingMaskIntoConstraints = false
        filtersScroll.heightAnchor.constraint(equalToConstant: 130).isActive = true

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

    // MARK: table

    func numberOfRows(in tableView: NSTableView) -> Int {
        filters.count
    }

    func tableView(
        _ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int
    ) -> NSView? {
        let (name, filter) = filters[row]
        let text = tableColumn?.identifier.rawValue == "name" ? name : filter
        let label = NSTextField(labelWithString: text)
        label.lineBreakMode = .byTruncatingTail
        return label
    }
}
