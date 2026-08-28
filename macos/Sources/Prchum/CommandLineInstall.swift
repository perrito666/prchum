import AppKit
import PrchumKit

extension AppDelegate {
    /// Prchum → Install Command…: puts `prchum` and `git-prchum` on the
    /// PATH at `/usr/local/bin`. Not `pr`, which it used to be called:
    /// that is POSIX's paginator, and /usr/local/bin comes first on the
    /// default PATH, so installing it there shadowed a standard tool.
    /// A plain copy is tried first; when the
    /// directory needs more rights than we have, macOS asks for them
    /// with the standard administrator prompt.
    @objc func installCommandLineTool(_ sender: Any?) {
        guard let source = Self.script(named: "prchum") else {
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = "Could not find the prchum script"
            alert.informativeText =
                "The app bundle is missing its prchum resource; "
                + "rebuild with `make app`, or install from a checkout with "
                + "`make install-cli`."
            alert.runModal()
            return
        }
        let target = "/usr/local/bin/prchum"
        var failure: String?
        do {
            try FileManager.default.createDirectory(
                atPath: "/usr/local/bin", withIntermediateDirectories: true)
            if FileManager.default.fileExists(atPath: target) {
                try FileManager.default.removeItem(atPath: target)
            }
            try FileManager.default.copyItem(atPath: source, toPath: target)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o755], ofItemAtPath: target)
        } catch {
            failure = escalatedInstall(source: source)
        }
        let alert = NSAlert()
        if let failure {
            alert.alertStyle = .warning
            alert.messageText = "prchum was not installed"
            alert.informativeText = failure
        } else {
            alert.alertStyle = .informational
            alert.messageText = "prchum installed"
            alert.informativeText =
                "\(target) is ready. From a terminal:\n\n"
                + "prchum — what git diff would show\n"
                + "prchum main — this branch against main\n"
                + "prchum 418 — review a pull request of the current repo\n"
                + "git prchum — the same, as a git command\n"
                + "prchum change.diff — a patch or exchange file"
        }
        alert.runModal()
    }

    /// The install rerun with administrator rights, via the standard
    /// system prompt. Returns a failure message, or nil on success.
    private func escalatedInstall(source: String) -> String? {
        let command =
            "install -d /usr/local/bin && install -m 0755 '\(source)' /usr/local/bin/pr"
        let escaped = command.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let script = NSAppleScript(
            source: "do shell script \"\(escaped)\" with administrator privileges")
        var error: NSDictionary?
        script?.executeAndReturnError(&error)
        guard let error else { return nil }
        // A canceled password prompt is a decision, not a failure worth
        // alarming about — but the alert still says nothing was installed.
        return error[NSAppleScript.errorMessage] as? String ?? "unknown error"
    }

    /// The bundled script — or, running from a checkout, the one in the
    /// repository (the build products live a few directories below it).
    private static func script(named name: String) -> String? {
        if let bundled = Bundle.main.resourceURL?.appendingPathComponent(name).path,
            FileManager.default.fileExists(atPath: bundled)
        {
            return bundled
        }
        var directory = Bundle.main.bundleURL.deletingLastPathComponent()
        for _ in 0..<6 {
            let candidate = directory.appendingPathComponent("scripts/pr")
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate.path
            }
            directory.deleteLastPathComponent()
        }
        return nil
    }

    /// Prchum → Open Themes Folder: where user theme files live —
    /// dropping a `<name>.json` there adds it to the Settings picker.
    @objc func openThemesFolder(_ sender: Any?) {
        let directory = URL(
            fileURLWithPath: (CoreConfig.defaultPath as NSString)
                .deletingLastPathComponent + "/themes")
        try? FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        NSWorkspace.shared.open(directory)
    }
}
