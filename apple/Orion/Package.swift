// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "Orion",
    platforms: [
        .macOS(.v14),
        .iOS(.v17)
    ],
    products: [
        .executable(name: "Orion", targets: ["Orion"])
    ],
    targets: [
        // Binary target for the Rust FFI XCFramework
        .binaryTarget(
            name: "MailFFI",
            path: "MailFFI.xcframework"
        ),
        .executableTarget(
            name: "Orion",
            dependencies: ["MailFFI"],
            path: "Sources",
            swiftSettings: [
                .define("DEBUG", .when(configuration: .debug))
            ]
        )
    ]
)
