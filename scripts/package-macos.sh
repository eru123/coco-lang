#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_NAME="coco"
PKG_VERSION="$(sed -n 's/^version\s*=\s*"\(.*\)"$/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1)"
ARCH="$(uname -m)"
case "${ARCH}" in
  x86_64) MAC_ARCH="x86_64" ;;
  arm64) MAC_ARCH="arm64" ;;
  *) echo "unsupported arch: ${ARCH}" >&2; exit 1 ;;
esac
DIST_DIR="${REPO_ROOT}/dist/macos-${MAC_ARCH}"
mkdir -p "${DIST_DIR}/payload/usr/local/bin"
cargo build --release --bin "${BIN_NAME}"
install -m 0755 "${REPO_ROOT}/target/release/${BIN_NAME}" "${DIST_DIR}/payload/usr/local/bin/${BIN_NAME}"
pkgbuild --root "${DIST_DIR}/payload" --identifier "com.coco-lang.cli" --version "${PKG_VERSION}" --install-location "/" "${DIST_DIR}/${BIN_NAME}-${PKG_VERSION}-${MAC_ARCH}.pkg"
rm -rf "${DIST_DIR}/payload"
echo "wrote ${DIST_DIR}/${BIN_NAME}-${PKG_VERSION}-${MAC_ARCH}.pkg"
if command -v hdiutil >/dev/null 2>&1; then
  DMG_TMP="${DIST_DIR}/dmg-tmp"
  mkdir -p "${DMG_TMP}"
  cp "${DIST_DIR}/${BIN_NAME}-${PKG_VERSION}-${MAC_ARCH}.pkg" "${DMG_TMP}/"
  hdiutil create -srcfolder "${DMG_TMP}" -volname "Coco ${PKG_VERSION}" -fs HFS+ -format UDZO "${DIST_DIR}/${BIN_NAME}-${PKG_VERSION}-${MAC_ARCH}.dmg" >/dev/null
  rm -rf "${DMG_TMP}"
  echo "wrote ${DIST_DIR}/${BIN_NAME}-${PKG_VERSION}-${MAC_ARCH}.dmg"
fi
