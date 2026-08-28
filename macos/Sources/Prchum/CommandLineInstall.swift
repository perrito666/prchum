import AppKit
import PrchumKit

extension AppDelegate {
    /// One file to install: where it comes from, where it goes, and the
    /// mode it lands with.
    private struct Installable {
        let name: String
        let source: String
        let destination: String
        let mode: Int
    }

    /// Prchum → Install Command-Line Tool…: puts `prchum`, `git-prchum`
    /// and their man pages under `/usr/local`.
    ///
    /// It used to install a command called `pr`, which is POSIX's
    /// paginator; `/usr/local/bin` precedes `/usr/bin` on the default
    /// PATH, so that shadowed a standard tool. Anyone carrying the old
    /// one can remove `/usr/local/bin/pr`.
    ///
    /// A plain copy is tried first. When the directories need more
    /// rights than the app has, macOS asks for them once, with the
    /// standard administrator prompt, for the whole set.
    @objc func installCommandLineTool(_ sender: Any?) {
        let wanted: [(name: String, subdirectory: String, destination: String, mode: Int)] = [
            ("prchum", "", "/usr/local/bin/prchum", 0o755),
            ("git-prchum", "", "/usr/local/bin/git-prchum", 0o755),
            ("prchum.1", "man", "/usr/local/share/man/man1/prchum.1", 0o644),
            ("git-prchum.1", "man", "/usr/local/share/man/man1/git-prchum.1", 0o644),
        ]

        var files: [Installable] = []
        for item in wanted {
            guard let source = Self.resource(named: item.name, subdirectory: item.subdirectory)
            else {
                present(
                    warning: "Could not find \(item.name)",
                    detail: "The app bundle is missing its \(item.name) resource; "
                        + "rebuild with `make app`, or install from a checkout with "
                        + "`make install-cli`.")
                return
            }
            files.append(
                Installable(
                    name: item.name, source: source, destination: item.destination,
                    mode: item.mode))
        }

        var failure: String?
        do {
            try copy(files)
        } catch {
            failure = escalatedInstall(files)
        }

        if let failure {
            present(warning: "Nothing was installed", detail: failure)
        } else {
            present(
                information: "prchum installed",
                detail: "/usr/local/bin/prchum is ready. From a terminal:\n\n"
                    + "prchum — what git diff would show\n"
                    + "prchum main — this branch against main\n"
                    + "prchum 418 — review a pull request of the current repo\n"
                    + "git prchum — the same, as a git command\n"
                    + "man prchum — the manual")
        }
    }

    private func copy(_ files: [Installable]) throws {
        for file in files {
            let directory = (file.destination as NSString).deletingLastPathComponent
            try FileManager.default.createDirectory(
                atPath: directory, withIntermediateDirectories: true)
            if FileManager.default.fileExists(atPath: file.destination) {
                try FileManager.default.removeItem(atPath: file.destination)
            }
            try FileManager.default.copyItem(atPath: file.source, toPath: file.destination)
            try FileManager.default.setAttributes(
                [.posixPermissions: file.mode], ofItemAtPath: file.destination)
        }
    }

    /// The install rerun with administrator rights, as one command so the
    /// password is asked for once. Returns a failure message, or nil.
    private func escalatedInstall(_ files: [Installable]) -> String? {
        var steps: [String] = []
        for file in files {
            let directory = (file.destination as NSString).deletingLastPathComponent
            let mode = String(format: "%04o", file.mode)
            steps.append("install -d '\(directory)'")
            steps.append("install -m \(mode) '\(file.source)' '\(file.destination)'")
        }
        let command = steps.joined(separator: " && ")
        let escaped = command.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        let script = NSAppleScript(
            source: "do shell script \"\(escaped)\" with administrator privileges")
        var error: NSDictionary?
        script?.executeAndReturnError(&error)
        guard let error else { return nil }
        // A cancelled password prompt is a decision; the alert says
        // nothing was installed and leaves it there.
        return error[NSAppleScript.errorMessage] as? String ?? "unknown error"
    }

    /// The bundled resource — or, running from a checkout, the file in
    /// the repository, which sits a few directories above the build
    /// products.
    private static func resource(named name: String, subdirectory: String) -> String? {
        if let bundled = Bundle.main.resourceURL?.appendingPathComponent(name).path,
            FileManager.default.fileExists(atPath: bundled)
        {
            return bundled
        }
        let relative =
            subdirectory.isEmpty ? "scripts/\(name)" : "scripts/\(subdirectory)/\(name)"
        var directory = Bundle.main.bundleURL.deletingLastPathComponent()
        for _ in 0..<6 {
            let candidate = directory.appendingPathComponent(relative)
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate.path
            }
            directory.deleteLastPathComponent()
        }
        return nil
    }

    private func present(warning message: String, detail: String) {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = message
        alert.informativeText = detail
        alert.runModal()
    }

    private func present(information message: String, detail: String) {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = message
        alert.informativeText = detail
        alert.runModal()
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
