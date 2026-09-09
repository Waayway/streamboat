#!/usr/bin/env bash
#
# build-dmg.sh -- assemble streamboat.app and wrap it in a DMG.
#
# Not run in this environment (no macOS here; see packaging/README.md's
# macOS section for what could and couldn't be verified locally: this
# script passes `bash -n` and shellcheck, nothing more). It only uses
# tools that ship with macOS itself (hdiutil, install_name_tool, iconutil)
# plus standard POSIX utilities, so it needs no `create-dmg` or Homebrew
# dependency -- "create-dmg-style" in the sense of producing the same kind
# of drag-to-Applications DMG that tool makes, hand-rolled with hdiutil.
#
# No code signing and no notarization in v1 (D-042): the resulting .app
# and .dmg are unsigned. Gatekeeper will refuse to open them normally;
# packaging/README.md documents the `xattr -d com.apple.quarantine` and
# right-click-Open workarounds a user needs. This script does not attempt
# to sign or notarize anything, and does not need
# APPLE_DEVELOPER_ID_CERTIFICATE/APPLE_NOTARIZATION_* secrets -- there are
# deliberately none of those in this repository's CI.
#
# Usage:
#   packaging/macos/build-dmg.sh \
#     --version 0.0.1 \
#     --streamboat-bin target/release/streamboat \
#     --streamboatd-bin target/release/streamboatd \
#     --libmpv-dylib vendor/libmpv/libmpv.2.dylib \
#     --icon packaging/macos/streamboat.icns \
#     --out dist/streamboat-0.0.1.dmg
#
# See .github/workflows/release.yml for exactly where each input path
# comes from on a real macOS CI runner (the pinned libmpv build per D-016,
# and an .icns generated from packaging/linux/io.github.waayway.streamboat.svg).

set -euo pipefail

APP_NAME="streamboat"
# The bundle identifier itself (io.github.waayway.streamboat, D-007) lives
# in packaging/macos/Info.plist, not here.

VERSION=""
STREAMBOAT_BIN=""
STREAMBOATD_BIN=""
LIBMPV_DYLIB=""
ICON_ICNS=""
OUT_DMG=""
WORK_DIR=""

usage() {
  cat <<'EOF'
Assemble streamboat.app (with a bundled libmpv) and wrap it in an unsigned DMG.

  --version VERSION           App version (CFBundleVersion/CFBundleShortVersionString)
  --streamboat-bin PATH        Path to the built `streamboat` binary
  --streamboatd-bin PATH       Path to the built `streamboatd` binary
  --libmpv-dylib PATH          Path to the pinned libmpv.*.dylib to bundle (D-016)
  --icon PATH                  Path to a prebuilt streamboat.icns
  --out PATH                   Output .dmg path
  --work-dir PATH              Scratch directory (default: a mktemp -d)
  --help                       Show this help and exit
EOF
}

log() {
  printf '[build-dmg] %s\n' "$1" >&2
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "build-dmg.sh: required tool not found: $1 (this script only runs on macOS)" >&2
    exit 1
  fi
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --streamboat-bin)
      STREAMBOAT_BIN="$2"
      shift 2
      ;;
    --streamboatd-bin)
      STREAMBOATD_BIN="$2"
      shift 2
      ;;
    --libmpv-dylib)
      LIBMPV_DYLIB="$2"
      shift 2
      ;;
    --icon)
      ICON_ICNS="$2"
      shift 2
      ;;
    --out)
      OUT_DMG="$2"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="$2"
      shift 2
      ;;
    --help | -h)
      usage
      exit 0
      ;;
    *)
      echo "build-dmg.sh: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

for required in VERSION STREAMBOAT_BIN STREAMBOATD_BIN LIBMPV_DYLIB ICON_ICNS OUT_DMG; do
  if [ -z "${!required}" ]; then
    echo "build-dmg.sh: missing required argument for ${required}" >&2
    usage >&2
    exit 2
  fi
done

require_tool hdiutil
require_tool install_name_tool
require_tool codesign

if [ -z "$WORK_DIR" ]; then
  WORK_DIR="$(mktemp -d)"
fi

APP_BUNDLE="${WORK_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_BUNDLE}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
FRAMEWORKS_DIR="${CONTENTS_DIR}/Frameworks"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

build_app_bundle() {
  log "assembling ${APP_BUNDLE}"
  rm -rf "$APP_BUNDLE"
  mkdir -p "$MACOS_DIR" "$FRAMEWORKS_DIR" "$RESOURCES_DIR"

  install -m 755 "$STREAMBOAT_BIN" "${MACOS_DIR}/streamboat"
  install -m 755 "$STREAMBOATD_BIN" "${MACOS_DIR}/streamboatd"
  install -m 644 "$LIBMPV_DYLIB" "${FRAMEWORKS_DIR}/$(basename "$LIBMPV_DYLIB")"
  install -m 644 "$ICON_ICNS" "${RESOURCES_DIR}/streamboat.icns"

  log "writing Info.plist (version ${VERSION})"
  sed "s/__VERSION__/${VERSION}/g" \
    "$(dirname "$0")/Info.plist" > "${CONTENTS_DIR}/Info.plist"
}

relink_libmpv() {
  local dylib_name
  dylib_name="$(basename "$LIBMPV_DYLIB")"

  log "pointing both binaries at @executable_path/../Frameworks/${dylib_name}"
  for bin in "${MACOS_DIR}/streamboat" "${MACOS_DIR}/streamboatd"; do
    # The exact existing load-command path depends on how the binary was
    # linked; `otool -L "$bin"` on a real build tells you what to pass as
    # the first argument here. This assumes the common case of a dylib
    # referenced by its bare install name (e.g. libmpv.2.dylib).
    install_name_tool \
      -change "${dylib_name}" "@executable_path/../Frameworks/${dylib_name}" \
      "$bin" || log "warning: install_name_tool found no matching load command in $(basename "$bin") -- check otool -L output"
  done
}

adhoc_sign() {
  # Not code signing for distribution (D-042: no Developer ID, no
  # notarization in v1) -- an ad hoc signature (`-s -`) only satisfies
  # macOS's requirement that arm64 binaries carry *some* signature to run
  # at all. It does nothing to satisfy Gatekeeper; packaging/README.md's
  # quarantine-removal workaround is still required.
  log "applying an ad hoc signature (not a Developer ID signature; see D-042)"
  codesign --force --deep --sign - "$APP_BUNDLE"
}

build_dmg() {
  local staging="${WORK_DIR}/dmg-staging"
  log "staging DMG contents in ${staging}"
  rm -rf "$staging"
  mkdir -p "$staging"
  cp -R "$APP_BUNDLE" "$staging/"
  ln -s /Applications "${staging}/Applications"

  mkdir -p "$(dirname "$OUT_DMG")"
  rm -f "$OUT_DMG"

  log "writing ${OUT_DMG}"
  hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$staging" \
    -ov \
    -format UDZO \
    "$OUT_DMG"
}

main() {
  build_app_bundle
  relink_libmpv
  adhoc_sign
  build_dmg
  log "done: ${OUT_DMG}"
  log "unsigned build (D-042): first launch needs Gatekeeper's right-click Open, or:"
  log "  xattr -d com.apple.quarantine /Applications/${APP_NAME}.app"
}

main
