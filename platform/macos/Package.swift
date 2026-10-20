// swift-tools-version:5.10
//
// Edgeway for macOS — menu bar app around the Rust core.
//
// `scripts/build-macos.sh` is the canonical build: it compiles the core, calls
// swiftc directly (so it works with only the Command Line Tools installed) and
// wraps the result into Edgeway.app. This manifest lets you open and build the
// same sources in Xcode or with `swift build` after building the core.

import PackageDescription

let coreLibrary = Context.packageDirectory + "/../../core/target/universal-release"

let package = Package(
    name: "Edgeway",
    platforms: [.macOS(.v13)],
    targets: [
        .systemLibrary(name: "CEdgeway", path: "Sources/CEdgeway"),
        .executableTarget(
            name: "Edgeway",
            dependencies: ["CEdgeway"],
            path: "Sources/Edgeway",
            swiftSettings: [.unsafeFlags(["-Osize"], .when(configuration: .release))],
            linkerSettings: [
                .unsafeFlags(["-L", coreLibrary, "-dead_strip"]),
                .linkedFramework("AppKit"),
                .linkedFramework("Carbon"),
                .linkedFramework("IOKit"),
                .linkedFramework("Network"),
                .linkedFramework("ServiceManagement"),
                .linkedFramework("SwiftUI"),
            ]
        ),
    ]
)
