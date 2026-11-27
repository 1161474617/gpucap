#!/usr/bin/env bash
set -euo pipefail

# 在 Apple Silicon Mac (arm64) 上编译并打包 GPUCap.app
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "[1/3] 编译 arm64 Release 二进制..."
swift build -c release --arch arm64

BIN_DIR="$(swift build -c release --arch arm64 --show-bin-path 2>/dev/null || true)"
BIN_PATH="$BIN_DIR/GPUCap"
if [ ! -f "$BIN_PATH" ]; then
  for candidate in \
    ".build/arm64-apple-macosx/release/GPUCap" \
    ".build/out/Products/Release/GPUCap" \
    ".build/release/GPUCap"; do
    if [ -f "$candidate" ]; then
      BIN_PATH="$candidate"
      break
    fi
  done
fi

if [ ! -f "$BIN_PATH" ]; then
  echo "未找到编译产物：$BIN_PATH"
  exit 1
fi

APP_DIR="$SCRIPT_DIR/dist/GPUCap.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

echo "[2/3] 组装 GPUCap.app 包结构..."
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/GPUCap"
chmod +x "$APP_DIR/Contents/MacOS/GPUCap"
cp "$SCRIPT_DIR/Resources/Info.plist" "$APP_DIR/Contents/Info.plist"
printf 'APPL????' > "$APP_DIR/Contents/PkgInfo"

echo "[3/3] 执行本地签名与归档..."
codesign --force --deep --sign - "$APP_DIR"

ZIP_PATH="$SCRIPT_DIR/dist/GPUCap-macos-arm64.zip"
rm -f "$ZIP_PATH"
ditto -c -k --sequesterRsrc --keepParent "$APP_DIR" "$ZIP_PATH"

echo "打包完成："
echo "  App 包：$APP_DIR"
echo "  压缩包：$ZIP_PATH"
