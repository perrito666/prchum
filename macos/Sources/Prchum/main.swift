import AppKit
import PrchumKit

// Apps launched from Finder inherit the minimal system PATH, which is
// missing Homebrew, cargo, and friends — exactly where gh, glab, and fj
// live. Merge the login shell's PATH before anything spawns a process.
adoptLoginShellPath()

// `--smoke-test` exercises the full Swift ↔ core round trip headlessly and
// exits; it is what CI runs, and a quick sanity check for humans.
if CommandLine.arguments.contains("--smoke-test") {
    // Top-level code runs on the main thread; assumeIsolated makes that
    // visible to the compiler.
    exit(MainActor.assumeIsolated { runSmokeTest() })
}

MainActor.assumeIsolated {
    let app = NSApplication.shared
    let delegate = AppDelegate()
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    app.run()
}
