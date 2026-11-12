#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
[[ $(uname -s) == Darwin ]] || { echo 'Execute em macOS com Xcode Command Line Tools'; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=11.0
export CARGO_TARGET_DIR="$(pwd)/target"
# Use the committed renditions in normal builds, including GitHub Actions.
# Re-exporting requires the separate Icon Composer app and is an explicit task.
if [[ "${SISSY_RENDER_ICONS:-0}" == 1 ]]; then
 python3 scripts/render-icon.py
fi
mode=${1:-universal}
case "$mode" in
 universal) targets=(aarch64-apple-darwin x86_64-apple-darwin);;
 arm64) targets=(aarch64-apple-darwin);;
 x86_64) targets=(x86_64-apple-darwin);;
 *) echo 'Uso: package-macos.sh [universal|arm64|x86_64]'; exit 1;;
esac
for target in "${targets[@]}"; do
 rustup target add "$target"
 cargo build --release --locked --target "$target"
done
out="dist/$mode"
app="$out/Sissy.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
if [[ "$mode" == universal ]]; then
 lipo -create target/{aarch64,x86_64}-apple-darwin/release/sissy -output "$app/Contents/MacOS/Sissy"
else
 cp "target/${targets[0]}/release/sissy" "$app/Contents/MacOS/Sissy"
fi
# Use the supplied PNG artwork to generate the standard ICNS sizes.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
sips -s format png "assets/Sissy-composed.png" --out "$tmp/sissy.svg.png" >/dev/null
mkdir -p "$tmp/Sissy.iconset"
for size in 16 32 128 256 512; do
 sips -z "$size" "$size" "$tmp/sissy.svg.png" --out "$tmp/Sissy.iconset/icon_${size}x${size}.png" >/dev/null
 double=$((size*2))
 sips -z "$double" "$double" "$tmp/sissy.svg.png" --out "$tmp/Sissy.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$tmp/Sissy.iconset" -o "$app/Contents/Resources/Sissy.icns"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Sissy</string>
<key>CFBundleIdentifier</key><string>dev.sissy.cleaner</string>
<key>CFBundleName</key><string>Sissy</string>
<key>CFBundleVersion</key><string>20261112.1</string>
<key>NSHumanReadableCopyright</key><string>© 2026 STRIKER Code Team</string>
<key>CFBundleDevelopmentRegion</key><string>pt</string>
<key>CFBundleLocalizations</key><array><string>pt</string><string>en</string><string>es</string><string>ja</string></array>
<key>CFBundleShortVersionString</key><string>0.2.0</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>Sissy</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSAppleEventsUsageDescription</key><string>Permitir que o Finder mova somente os arquivos confirmados ao Lixo.</string>
</dict></plist>
PLIST
# Xcode 26+ compiles Icon Composer layers into Assets.car.
# Command Line Tools can still ship a working app with the ICNS fallback.
if xcrun --find actool >/dev/null 2>&1; then
 xcrun actool icon_file/SISSY_ICON_MAC.icon \
  --compile "$app/Contents/Resources" \
  --platform macosx --minimum-deployment-target "$MACOSX_DEPLOYMENT_TARGET" \
  --target-device mac --app-icon SISSY_ICON_MAC \
  --output-partial-info-plist "$tmp/icon-info.plist"
 /usr/libexec/PlistBuddy -c 'Add :CFBundleIconName string SISSY_ICON_MAC' "$app/Contents/Info.plist"
else
 echo 'actool ausente (Xcode completo). O app usa Sissy.icns.'
fi
# Ad-hoc signing enables local Apple Silicon execution; this is NOT Developer ID or notarization.
codesign --force --sign - "$app"
mkdir -p "$tmp/image"
cp -R "$app" "$tmp/image/"
ln -s /Applications "$tmp/image/Applications"
hdiutil create -volname Sissy -srcfolder "$tmp/image" -ov -format UDZO "$out/Sissy-$mode.dmg"
echo "Gerado: $out (assinatura ad-hoc; sem notarização)"
