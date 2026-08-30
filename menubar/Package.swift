// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ModelMuxBar",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(
            name: "ModelMuxBar",
            path: "ModelMuxBar"
        )
    ]
)
