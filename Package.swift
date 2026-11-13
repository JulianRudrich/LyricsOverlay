// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "LyricsOverlay",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "LyricsOverlay", targets: ["LyricsOverlay"])],
    targets: [
        .target(name: "LyricsCore"),
        .executableTarget(name: "LyricsOverlay", dependencies: ["LyricsCore"]),
        .executableTarget(name: "LyricsCoreChecks", dependencies: ["LyricsCore"], path: "Checks")
    ]
)
