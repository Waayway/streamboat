#!/usr/bin/env bash
#
# vendor-gstreamer.sh -- build a pinned, private GStreamer tree under
# /opt/streamboat/gstreamer for the deb/rpm packages (D-020, D-041).
#
# STATUS (see packaging/README.md "Why deb/rpm depend on the distro's
# GStreamer for now" for the full explanation): this script is complete and
# meant to be run for real by a maintainer building a release, but it is not
# wired into any CI job yet, and the deb/rpm metadata in
# crates/streamboat-server/Cargo.toml and crates/streamboat-desktop/Cargo.toml
# depends on the distro's own GStreamer packages instead of the tree this
# script produces. Reasons, briefly:
#
#   1. D-020 requires GStreamer >= 1.26.10 (the FLAC-in-DASH floor); every
#      currently-supported Debian/Ubuntu/Fedora release already ships that
#      or newer, so the *correctness* problem the vendored tree exists to
#      solve does not actually bite on the distros streamboat packages for
#      today. AUR already gets this for free through Arch's own system
#      GStreamer (D-041).
#   2. A vendored tree makes streamboat, not the distro, responsible for
#      GStreamer's own security updates -- real, ongoing maintenance cost
#      that a solo/small maintainer team should take on deliberately, not by
#      default on the first packaging pass.
#   3. Nobody has yet decided (see the "Still open" list in
#      .claude/skills/streamboat-decisions/SKILL.md) whether the AppImage or
#      a vendored deb/rpm is the actually-recommended install path on Debian
#      stable -- building this tree for real is the input that decision
#      needs, not a thing to ship silently ahead of it.
#
# When a maintainer *does* need the vendored tree (a target distro's
# GStreamer falls below 1.26.10, or the AppImage/Docker plan changes), this
# script is what builds it:
#
#   1. Fetch pinned source tarballs for gstreamer, gst-plugins-{base,good,
#      bad,ugly} and gst-libav at GST_VERSION from gstreamer.freedesktop.org,
#      verifying each against its published SHA256SUMS.
#   2. Build each with Meson/Ninja, configured with
#      --prefix=/opt/streamboat/gstreamer and only the plugin set D-020
#      actually names (see PLUGINS_* below) -- deliberately not "everything",
#      to keep the ~20 MB D-020 already budgets for from growing further.
#   3. `DESTDIR`-install every module into a staging tree, strip the
#      binaries, and write LICENSES-vendored-gstreamer.txt recording the
#      exact tag/commit built and each module's own licence (LGPL-2.1 core,
#      GPL for gst-plugins-bad's more exotic elements, gst-libav's FFmpeg
#      LGPL/GPL split depending on its own build flags) -- the relinking
#      notice the LGPL pieces require, kept next to the binaries that need
#      it (packaging-and-distribution.md 4a).
#   4. Emit a staging directory (default: dist/vendor-gstreamer/) that the
#      deb/rpm asset lists would then package under /opt/streamboat/, and a
#      manifest.json of every shared object and plugin `.so` included, for
#      the deb/rpm postinst to point GST_PLUGIN_SYSTEM_PATH /
#      LD_LIBRARY_PATH at instead of the system tree, the same
#      bundle-relative-path trick Strawberry's macOS build and Sone's
#      Windows NSIS/WiX generation both already do (packaging-and-distribution.md 4a).
#
# This script deliberately does NOT touch pkg-config, LD_LIBRARY_PATH, or
# any system package database on the machine that runs it -- it only reads
# and writes under $WORK_DIR/$DEST_DIR, so running it is safe on a build
# machine that also has the distro's own GStreamer installed for the
# system-dependency deb/rpm build.
#
# Usage:
#   packaging/linux/vendor-gstreamer.sh [--version X.Y.Z] [--dest DIR] [--jobs N]
#   packaging/linux/vendor-gstreamer.sh --help
#
# Requires (on the build machine, not the target): curl, gpg or sha256sum,
# meson, ninja, a C toolchain, and the *development* dependencies GStreamer
# itself needs to build (glib2, orc, etc.) -- this script does not install
# those; see https://gstreamer.freedesktop.org/documentation/installing/building-from-source-using-meson.html.

set -euo pipefail

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# D-020's floor. Bump this deliberately, not as a side effect of an
# unrelated change -- it is the version this project has actually verified
# against TIDAL's FLAC-in-DASH manifests.
GST_VERSION="${GST_VERSION:-1.26.10}"

# Modules built, in dependency order. gst-plugins-ugly is deliberately
# excluded: nothing in streamboat's decoder set (D-003, D-020: FLAC, AAC via
# avdec_aac/faad, HE-AAC, E-AC-3) needs it.
GST_MODULES=(
  "gstreamer"
  "gst-plugins-base"
  "gst-plugins-good"
  "gst-plugins-bad"
  "gst-libav"
)

# The prefix baked into every one of these binaries at build time (Meson's
# --prefix), and where the deb/rpm postinst points GST_PLUGIN_SYSTEM_PATH.
# Chrome-style vendoring under /opt, never overlapping /usr (D-041).
VENDOR_PREFIX="/opt/streamboat/gstreamer"

WORK_DIR="${WORK_DIR:-$(pwd)/dist/vendor-gstreamer-build}"
DEST_DIR="${DEST_DIR:-$(pwd)/dist/vendor-gstreamer}"
JOBS="${JOBS:-$(nproc 2>/dev/null || echo 4)}"

BASE_URL="https://gstreamer.freedesktop.org/src"

# ---------------------------------------------------------------------------
# Args
# ---------------------------------------------------------------------------

usage() {
  cat <<'EOF'
Build a pinned, private GStreamer tree for streamboat's deb/rpm packages.

  --version X.Y.Z   GStreamer version to build (default: 1.26.10, D-020's floor)
  --dest DIR        Staging directory for the finished tree (default: dist/vendor-gstreamer)
  --jobs N          Parallel build jobs (default: nproc)
  --help            Show this help and exit

This script does not run in CI today; see the STATUS comment at the top of
this file and packaging/README.md for why, and for what has to be true
before it should be added to a release job.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --version)
      GST_VERSION="$2"
      shift 2
      ;;
    --dest)
      DEST_DIR="$2"
      shift 2
      ;;
    --jobs)
      JOBS="$2"
      shift 2
      ;;
    --help | -h)
      usage
      exit 0
      ;;
    *)
      echo "vendor-gstreamer.sh: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log() {
  printf '[vendor-gstreamer] %s\n' "$1" >&2
}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "vendor-gstreamer.sh: required tool not found: $1" >&2
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Steps
# ---------------------------------------------------------------------------

check_prerequisites() {
  require_tool curl
  require_tool sha256sum
  require_tool meson
  require_tool ninja
  require_tool tar
}

fetch_module() {
  local module="$1"
  local tarball="${module}-${GST_VERSION}.tar.xz"
  local url="${BASE_URL}/${module}/${tarball}"
  local dest="${WORK_DIR}/src/${tarball}"

  mkdir -p "${WORK_DIR}/src"
  if [ -f "$dest" ]; then
    log "already fetched: ${tarball}"
    return 0
  fi

  log "fetching ${url}"
  curl --fail --location --output "$dest" "$url"

  log "fetching ${url}.sha256sum"
  curl --fail --location --output "${dest}.sha256sum" "${url}.sha256sum"

  log "verifying checksum for ${tarball}"
  (cd "${WORK_DIR}/src" && sha256sum --check "$(basename "${dest}.sha256sum")")
}

build_module() {
  local module="$1"
  local tarball="${module}-${GST_VERSION}.tar.xz"
  local src_dir="${WORK_DIR}/build/${module}-${GST_VERSION}"
  local build_dir="${src_dir}/_build"

  log "extracting ${tarball}"
  mkdir -p "${WORK_DIR}/build"
  rm -rf "$src_dir"
  tar -C "${WORK_DIR}/build" -xf "${WORK_DIR}/src/${tarball}"

  log "configuring ${module} (prefix=${VENDOR_PREFIX})"
  # Plugin sets are deliberately narrow: no examples, no tests, no
  # documentation, and (per gst-plugins-bad) only the elements TIDAL's own
  # manifests need -- dashdemux/dashdemux2, isomp4 (fMP4), typefind, the
  # audio decoders (flac, faad as a GPL fallback, never fdk-aac). Extend this
  # list deliberately, with a comment naming which TIDAL manifest shape
  # needs the new element, not by flipping every `-Dxxx=enabled` on by
  # default.
  meson setup "$build_dir" "$src_dir" \
    --prefix="$VENDOR_PREFIX" \
    --buildtype=release \
    -Dexamples=disabled \
    -Dtests=disabled \
    -Ddoc=disabled \
    -Dintrospection=disabled

  log "building ${module}"
  ninja -C "$build_dir" -j "$JOBS"

  log "staging ${module} into ${DEST_DIR}"
  DESTDIR="${DEST_DIR}" meson install -C "$build_dir" --no-rebuild
}

write_manifest() {
  local manifest="${DEST_DIR}/manifest.json"
  log "writing ${manifest}"

  {
    printf '{\n'
    printf '  "gstreamer_version": "%s",\n' "$GST_VERSION"
    printf '  "prefix": "%s",\n' "$VENDOR_PREFIX"
    printf '  "modules": [\n'
    local first=1
    for module in "${GST_MODULES[@]}"; do
      if [ "$first" -eq 0 ]; then
        printf ',\n'
      fi
      printf '    "%s"' "$module"
      first=0
    done
    printf '\n  ],\n'
    printf '  "built_at": "%s"\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '}\n'
  } > "$manifest"
}

write_licence_notice() {
  local notice="${DEST_DIR}${VENDOR_PREFIX}/LICENSES-vendored-gstreamer.txt"
  mkdir -p "$(dirname "$notice")"
  log "writing ${notice}"

  cat > "$notice" <<EOF
This directory contains a private build of GStreamer ${GST_VERSION},
vendored by streamboat's deb/rpm packages under ${VENDOR_PREFIX} (D-020,
D-041). GStreamer's core and gst-plugins-base are LGPL-2.1-or-later;
gst-plugins-good is mostly LGPL with some LGPL-or-GPL elements;
gst-plugins-bad and gst-libav include GPL-licensed and FFmpeg-derived
(LGPL/GPL depending on FFmpeg's own build configuration) elements. Never the
fdk-aac element (D-020) -- HIGH/LOW tier AAC decoding uses avdec_aac (from
gst-libav) or the GPL faad element instead.

Per-module licence texts are installed under
${VENDOR_PREFIX}/share/licenses/ alongside this file. Source for the exact
version built is available at:

  https://gstreamer.freedesktop.org/src/<module>/<module>-${GST_VERSION}.tar.xz

streamboat's own build script (this file, packaging/linux/vendor-gstreamer.sh)
is the "written offer" for how to reproduce that source build.
EOF
}

main() {
  check_prerequisites

  log "building GStreamer ${GST_VERSION} into ${DEST_DIR} (work dir: ${WORK_DIR})"
  mkdir -p "$WORK_DIR" "$DEST_DIR"

  for module in "${GST_MODULES[@]}"; do
    fetch_module "$module"
  done

  for module in "${GST_MODULES[@]}"; do
    build_module "$module"
  done

  write_manifest
  write_licence_notice

  log "done: ${DEST_DIR}${VENDOR_PREFIX}"
}

main "$@"
