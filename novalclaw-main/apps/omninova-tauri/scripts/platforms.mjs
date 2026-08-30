import process from "node:process";

export const hostMacTarget =
  process.arch === "arm64" ? "aarch64-apple-darwin" : "x86_64-apple-darwin";

export const platformCommands = {
  desktop: {
    category: "desktop",
    description: "Build the desktop app for the current host target.",
    args: ["build"],
  },
  linux: {
    category: "desktop",
    description: "Build Linux desktop bundle for x86_64.",
    target: "x86_64-unknown-linux-gnu",
    args: ["build", "--target", "x86_64-unknown-linux-gnu"],
  },
  "linux:arm64": {
    category: "desktop",
    description: "Build Linux desktop bundle for ARM64.",
    target: "aarch64-unknown-linux-gnu",
    args: ["build", "--target", "aarch64-unknown-linux-gnu"],
  },
  macos: {
    category: "desktop",
    description: "Build macOS desktop bundle for the current Mac architecture.",
    target: hostMacTarget,
    args: ["build", "--target", hostMacTarget],
  },
  "macos:intel": {
    category: "desktop",
    description: "Build macOS desktop bundle for Intel Macs.",
    target: "x86_64-apple-darwin",
    args: ["build", "--target", "x86_64-apple-darwin"],
  },
  "macos:apple": {
    category: "desktop",
    description: "Build macOS desktop bundle for Apple Silicon Macs.",
    target: "aarch64-apple-darwin",
    args: ["build", "--target", "aarch64-apple-darwin"],
  },
  windows: {
    category: "desktop",
    description: "Build Windows desktop bundle for x86_64.",
    target: "x86_64-pc-windows-msvc",
    args: ["build", "--target", "x86_64-pc-windows-msvc"],
  },
  "windows:arm64": {
    category: "desktop",
    description: "Build Windows desktop bundle for ARM64.",
    target: "aarch64-pc-windows-msvc",
    args: ["build", "--target", "aarch64-pc-windows-msvc"],
  },
  android: {
    category: "mobile",
    description: "Build the Android app bundle.",
    args: ["android", "build"],
  },
  "android:init": {
    category: "mobile",
    description: "Initialize the Android project files.",
    args: ["android", "init"],
  },
  ios: {
    category: "mobile",
    description: "Build the iOS app project.",
    args: ["ios", "build"],
  },
  "ios:init": {
    category: "mobile",
    description: "Initialize the iOS project files.",
    args: ["ios", "init"],
  },
};

export const desktopPlatformOrder = [
  "linux",
  "linux:arm64",
  "macos:intel",
  "macos:apple",
  "windows",
  "windows:arm64",
];
