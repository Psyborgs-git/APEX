#!/bin/bash
# Build script for APEX Terminal across macOS, Windows, and Linux

echo "⚡ Starting APEX multi-platform build..."

# Ensure we're in the project root
cd "$(dirname "$0")/.."

# Install dependencies if needed
echo "Installing JS dependencies..."
pnpm install

echo "Building for macOS (Apple Silicon & Intel)..."
# This requires cargo-lipo or similar if cross-compiling, but standard Tauri build uses host
cd apex-tauri
cargo tauri build --target universal-apple-darwin

# Note: Cross-compiling Windows/Linux from macOS requires specific toolchains or using GitHub Actions.
echo "Building for Windows (requires MinGW/cross-compilation or Docker)..."
# cargo tauri build --target x86_64-pc-windows-msvc

echo "Building for Linux (requires cross-compilation toolchain or Docker)..."
# cargo tauri build --target x86_64-unknown-linux-gnu

echo "✅ Build script execution complete."
echo "For true cross-platform builds from a single machine, we recommend using GitHub Actions or a Docker-based cross-compilation setup."
