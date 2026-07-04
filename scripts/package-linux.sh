#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
P_NAME="coco"
BIN_NAME="coco"
PKG_VERSION="$(sed -n 's/^version\s*=\s*"\(.*\)"$/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1)"
ARCH="$(uname -m)"
case "${ARCH}" in
  x86_64) DEB_ARCH="amd64" ;;
  aarch64) DEB_ARCH="arm64" ;;
  *) echo "unsupported arch: ${ARCH}" >&2; exit 1 ;;
esac
DIST_DIR="${REPO_ROOT}/dist/linux-${DEB_ARCH}"
mkdir -p "${DIST_DIR}/tmp/DEBIAN" "${DIST_DIR}/tmp/usr/bin"
cargo build --release --bin "${BIN_NAME}"
install -m 0755 "${REPO_ROOT}/target/release/${BIN_NAME}" "${DIST_DIR}/tmp/usr/bin/${BIN_NAME}"
cat > "${DIST_DIR}/tmp/DEBIAN/control" <<EOF
Package: ${P_NAME}
Version: ${PKG_VERSION}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Maintainer: eru123 <jericho@skiddph.com>
Description: Coco bytecode VM and CLI
EOF
dpkg-deb --build --root-owner-group "${DIST_DIR}/tmp" "${DIST_DIR}/${P_NAME}_${PKG_VERSION}_${DEB_ARCH}.deb"
rm -rf "${DIST_DIR}/tmp"
echo "wrote ${DIST_DIR}/${P_NAME}_${PKG_VERSION}_${DEB_ARCH}.deb"
