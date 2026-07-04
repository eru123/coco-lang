#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
BIN_NAME="coco"
INSTALL_PATH="/usr/local/bin/${BIN_NAME}"
echo "building ${BIN_NAME}..."
cargo build --release --bin "${BIN_NAME}" --quiet
install -m 0755 "${REPO_ROOT}/target/release/${BIN_NAME}" "${INSTALL_PATH}"
echo "installed to ${INSTALL_PATH}"
"${INSTALL_PATH}" --version
