#!/usr/bin/env bash
# UNIT3D-Installer bootstrap (Rust edition).
#
# Downloads a single static `unit3d-installer` binary from the latest
# GitHub Release, verifies its SHA-256 checksum, and runs it.
# Replaces the legacy `install.sh` + `ubuntu.sh` + `box.json`/PHAR chain.

set -euo pipefail

REPO="InfinityHD-Net/UNIT3D-Installer"

if [[ $EUID -ne 0 ]]; then
    echo "ERROR: Please run as root (sudo ./install.sh)" >&2
    exit 1
fi

# Ensure we have curl + tar.
if ! command -v curl >/dev/null 2>&1; then
    apt-get -y update
    apt-get -y install -y ca-certificates curl tar
fi

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)   ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) echo "ERROR: Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

# Prefer a specific version if the caller set UNIT3D_INSTALLER_VERSION,
# otherwise pull the latest release. GitHub's "latest" magic only works in
# the `releases/latest/download/...` form (it redirects to the newest
# release's asset); `releases/download/latest/...` would treat "latest" as a
# literal tag name and 404.
VERSION="${UNIT3D_INSTALLER_VERSION:-latest}"
if [[ "${VERSION}" == "latest" ]]; then
    URL="https://github.com/${REPO}/releases/latest/download/unit3d-installer-${ARCH}.tar.gz"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/unit3d-installer-${ARCH}.tar.gz"
fi
SUM_URL="${URL}.sha256"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $URL..."
curl -fsSL --retry 3 --connect-timeout 15 "$URL" -o "$TMP/unit3d-installer.tar.gz"
curl -fsSL --retry 3 --connect-timeout 15 "$SUM_URL" -o "$TMP/unit3d-installer.tar.gz.sha256" 2>/dev/null || true

# Integrity check: verify the tarball against the published checksum. If the
# checksum file is unavailable the install refuses to continue rather than
# running an unverified binary.
EXPECTED_SUM="$(awk '{print $1}' "$TMP/unit3d-installer.tar.gz.sha256" 2>/dev/null || true)"
if [[ -z "${EXPECTED_SUM}" ]]; then
    echo "ERROR: checksum for the release binary is unavailable; refusing to install unverified binary." >&2
    exit 1
fi
ACTUAL_SUM="$(sha256sum "$TMP/unit3d-installer.tar.gz" | awk '{print $1}')"
if [[ "${EXPECTED_SUM}" != "${ACTUAL_SUM}" ]]; then
    echo "ERROR: checksum mismatch for $URL" >&2
    echo "  expected: ${EXPECTED_SUM}" >&2
    echo "  actual:   ${ACTUAL_SUM}" >&2
    exit 1
fi
echo "Checksum verified (${ACTUAL_SUM})"

tar -xzf "$TMP/unit3d-installer.tar.gz" -C "$TMP"
install -m 0755 "$TMP/unit3d-installer" /usr/local/bin/unit3d-installer

# Smoke-check the freshly installed binary before handing off.
if ! /usr/local/bin/unit3d-installer --version >/dev/null 2>&1; then
    echo "ERROR: installed binary failed a self-check" >&2
    exit 1
fi

echo "Starting UNIT3D installer..."
exec /usr/local/bin/unit3d-installer "$@"
