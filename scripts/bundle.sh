#!/bin/sh
# Builds dist/Termi.app + Termi.zip and stamps version/sha into Casks/termi.rb.
set -e
cd "$(dirname "$0")/.."
cargo build --release
APP=dist/Termi.app
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/termi "$APP/Contents/MacOS/termi"
cp assets/Termi.icns "$APP/Contents/Resources/Termi.icns"
VERSION=${RELEASE_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Termi</string>
  <key>CFBundleIdentifier</key><string>dev.termi.app</string>
  <key>CFBundleExecutable</key><string>termi</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>Termi</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
if [ -n "${SIGNING_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"
(cd dist && rm -f Termi.zip && zip -qry Termi.zip Termi.app)
SHA=$(shasum -a 256 dist/Termi.zip | cut -d' ' -f1)
sed -i '' -e "s/version \".*\"/version \"$VERSION\"/" -e "s/sha256 \".*\"/sha256 \"$SHA\"/" Casks/termi.rb
echo "dist/Termi.zip $SHA -> upload to release v$VERSION, commit Casks/termi.rb"
