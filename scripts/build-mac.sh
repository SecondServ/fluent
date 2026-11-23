#!/usr/bin/env bash
# 编译 Fluent.app 并用固定证书签名（固定签名 → 重新编译后「辅助功能」授权不会丢）。
# 用法：bash scripts/build-mac.sh [debug|release]
#   签名证书默认沿用本机的 "Leadership Dev"，可用环境变量 FLUENT_SIGN_ID 覆盖；
#   找不到证书时退回 ad-hoc 签名（每次重编译后需要重新授权辅助功能）。
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env" 2>/dev/null || true

MODE="${1:-release}"
SIGN_ID="${FLUENT_SIGN_ID:-Leadership Dev}"

if [[ "$MODE" == "debug" ]]; then
  npx tauri build --debug --bundles app --no-sign
  APP="src-tauri/target/debug/bundle/macos/Fluent.app"
else
  npx tauri build --bundles app --no-sign
  APP="src-tauri/target/release/bundle/macos/Fluent.app"
fi

if security find-identity -p codesigning | grep -q "\"$SIGN_ID\""; then
  codesign --force --deep --sign "$SIGN_ID" "$APP"
  echo "✓ 已用「$SIGN_ID」签名"
else
  codesign --force --deep --sign - "$APP"
  echo "⚠ 没找到证书「$SIGN_ID」，已 ad-hoc 签名（重编译后需重新授权辅助功能）"
fi

mkdir -p build
rm -rf build/Fluent.app
cp -R "$APP" build/Fluent.app
echo "✓ build/Fluent.app"
