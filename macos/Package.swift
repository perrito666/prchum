// swift-tools-version:5.10
import Foundation
import PackageDescription

// Resolve the Rust static library relative to this file so builds work from
// any working directory (and from editor tooling).
let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let coreLibDir = "\(packageDir)/../core/target/release"

let package = Package(
    name: "Prchum",
    platforms: [.macOS(.v14)],
    targets: [
        // The generated C header exposed as a Clang module; no code.
        .target(name: "CPrchum"),
        // Safe Swift API over the C ABI — the only place pointers appear.
        .target(name: "PrchumKit", dependencies: ["CPrchum"]),
        // The app: windows, views, menus. Ordinary Swift, no FFI.
        .executableTarget(
            name: "Prchum",
            dependencies: ["PrchumKit"],
            linkerSettings: [
                .unsafeFlags(["-L\(coreLibDir)", "-lprchum"])
            ]
        ),
    ]
)
