// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "CodexMux",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "CodexMux",
            path: "CodexMux"
        )
    ]
)
