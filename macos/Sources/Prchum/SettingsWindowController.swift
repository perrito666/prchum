import AppKit
import PrchumKit

/// Settings: appearance and theme, written through to config.json (the
/// file stays hand-editable; unknown keys survive every save).
@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    private let appearancePicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private let themePicker = NSPopUpButton(frame: .zero, pullsDown: false)
    private let onChange: () -> Void

    var onClose: (() -> Void)?

    init(onChange: @escaping () -> Void) {
        self.onChange = onChange
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 380, height: 140),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false)
        window.title = "Settings"
        super.init(window: window)
        window.delegate = self

        appearancePicker.addItems(withTitles: ["System", "Light", "Dark"])
        appearancePicker.target = self
        appearancePicker.action = #selector(appearanceChanged(_:))

        themePicker.addItems(withTitles: Self.themeNames())
        themePicker.target = self
        themePicker.action = #selector(themeChanged(_:))

        let config = CoreConfig()
        appearancePicker.selectItem(at: Int(config.appearance.rawValue))
        let currentTheme = config.theme.isEmpty ? "default" : config.theme
        themePicker.selectItem(withTitle: currentTheme)
        if themePicker.selectedItem == nil {
            themePicker.selectItem(at: 0)
        }

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
        ])
        grid.rowSpacing = 10
        grid.column(at: 0).xPlacement = .trailing
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
        var names = ["default", "high-contrast"]
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
}
