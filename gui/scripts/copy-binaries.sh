#!/usr/bin/env bash
# Copy `proxy` and `client` binaries (built at the repo root) into the
# Tauri release output directory so they ship next to `omniproxy-gui.exe`.
#
# Usage:
#   copy-binaries.sh                       # auto-detect target dir
#   copy-binaries.sh x86_64-pc-windows-msvc
#   copy-binaries.sh x86_64-pc-windows-gnu
#   copy-binaries.sh --target x86_64-pc-windows-msvc
#   copy-binaries.sh --target=x86_64-pc-windows-msvc
#
# Source location (where `cargo build -p <crate> --release --target <triple>`
# drops binaries):
#   <repo>/target/<triple>/release/proxy[.exe]
#   <repo>/target/<triple>/release/client[.exe]
#
# Destination: the matching Tauri output directory + bundle/resources/.
#
# Exit codes:
#   0 = copied successfully (or already present)
#   1 = proxy/client binaries not found at the expected source location
#   2 = Tauri output directory does not exist (build first)

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

# Parse optional --target argument. Use an index-based loop because
# `shift` inside a `for arg in "$@"` body does NOT advance the loop
# iterator — it silently leaves the next arg unconsumed, and the bare
# `*)` fallback then misreads the consumed flag as a triple.
TARGET_TRIPLE=""
i=1
while [ "${i}" -le "$#" ]; do
  eval "arg=\${${i}}"
  case "${arg}" in
    --target=*) TARGET_TRIPLE="${arg#--target=}" ;;
    --target)
      i=$((i + 1))
      eval "TARGET_TRIPLE=\${${i}:-}"
      ;;
    --help|-h)
      sed -n '2,18p' "$0"
      exit 0
      ;;
    --) ;;  # pnpm's arg separator — ignore
    *)
      # bare triple (e.g. "x86_64-pc-windows-msvc")
      if [[ -z "${TARGET_TRIPLE}" && "${arg}" == *-* ]]; then
        TARGET_TRIPLE="${arg}"
      fi
      ;;
  esac
  i=$((i + 1))
done

SRC_DIR_DEFAULT="${REPO_ROOT}/target/release"
SRC_DIR_TRIPLE="${REPO_ROOT}/target/${TARGET_TRIPLE}/release"
SRC_DIR=""

if [[ -n "${TARGET_TRIPLE}" && -d "${SRC_DIR_TRIPLE}" ]]; then
  SRC_DIR="${SRC_DIR_TRIPLE}"
elif [[ -d "${SRC_DIR_DEFAULT}" ]]; then
  SRC_DIR="${SRC_DIR_DEFAULT}"
else
  echo "[copy-binaries] ERR: no source binary dir found." >&2
  echo "[copy-binaries] Tried: ${SRC_DIR_TRIPLE} and ${SRC_DIR_DEFAULT}" >&2
  if [[ -n "${TARGET_TRIPLE}" ]]; then
    echo "[copy-binaries] Build with: cargo build -p proxy -p client --release --target ${TARGET_TRIPLE}" >&2
  else
    echo "[copy-binaries] Build with: cargo build -p proxy -p client --release" >&2
  fi
  exit 1
fi

DEST_DIR_TRIPLE=""
if [[ -n "${TARGET_TRIPLE}" ]]; then
  DEST_DIR_TRIPLE="${REPO_ROOT}/gui/src-tauri/target/${TARGET_TRIPLE}/release"
else
  for triple in x86_64-pc-windows-msvc x86_64-pc-windows-gnu aarch64-pc-windows-msvc \
                x86_64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin; do
    candidate="${REPO_ROOT}/gui/src-tauri/target/${triple}/release"
    if [[ -d "${candidate}" ]]; then
      DEST_DIR_TRIPLE="${candidate}"
      break
    fi
  done
fi
DEST_DIR_DEFAULT="${REPO_ROOT}/gui/src-tauri/target/release"

copy_one() {
  local name="$1"
  local src="${SRC_DIR}/${name}"
  if [[ ! -f "${src}" ]]; then
    echo "[copy-binaries] WARN: source ${src} not found, skipping" >&2
    return 1
  fi
  local dests=()
  [[ -d "${DEST_DIR_TRIPLE}" ]] && dests+=("${DEST_DIR_TRIPLE}")
  [[ -d "${DEST_DIR_DEFAULT}" ]] && dests+=("${DEST_DIR_DEFAULT}")
  if [[ "${#dests[@]}" -eq 0 ]]; then
    echo "[copy-binaries] ERR: no destination directory exists." >&2
    echo "[copy-binaries] Build the GUI first: pnpm tauri build" >&2
    exit 2
  fi
  for dest in "${dests[@]}"; do
    cp -f "${src}" "${dest}/${name}"
    echo "[copy-binaries] ${src} -> ${dest}/${name}"
  done
  # Also drop into bundle resources/ if it exists (MSI/NSIS pick it up).
  local base
  if [[ -n "${DEST_DIR_TRIPLE}" ]]; then
    base="$(dirname "${DEST_DIR_TRIPLE}")"
  else
    base="$(dirname "${DEST_DIR_DEFAULT}")"
  fi
  local resources_dir="${base}/release/bundle/resources"
  if [[ -d "${resources_dir}" ]]; then
    cp -f "${src}" "${resources_dir}/${name}"
    echo "[copy-binaries] ${src} -> ${resources_dir}/${name}"
  fi
}

echo "[copy-binaries] source: ${SRC_DIR}"
echo "[copy-binaries] target: ${DEST_DIR_TRIPLE:-${DEST_DIR_DEFAULT}}"

failed=0
copy_one proxy.exe || copy_one proxy || failed=1
copy_one client.exe || copy_one client || failed=1

if [[ "${failed}" -ne 0 ]]; then
  echo "[copy-binaries] Some binaries were not found." >&2
  if [[ -n "${TARGET_TRIPLE}" ]]; then
    echo "[copy-binaries] Build them first: cargo build -p proxy -p client --release --target ${TARGET_TRIPLE}" >&2
  else
    echo "[copy-binaries] Build them first: cargo build -p proxy -p client --release" >&2
  fi
  exit 1
fi
echo "[copy-binaries] done."
