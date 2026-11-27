// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GPUCap",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(name: "GPUCap", targets: ["GPUCap"])
    ],
    targets: [
        .executableTarget(
            name: "GPUCap",
            path: "Sources/GPUCap"
        )
    ]
)
