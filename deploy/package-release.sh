#!/usr/bin/env bash
#
# Package a built IronMUD release into an installable tarball.
#
# Usage:
#   deploy/package-release.sh [--version v1.2.3] [--target <triple>] [--out dist]
#
# Expects `cargo build --release [--target <triple>]` to have run already.
# Produces:
#   <out>/ironmud-<arch>-linux.tar.gz
#   <out>/ironmud-<arch>-linux.tar.gz.sha256
#
# Asset names carry no version so the "latest release" download URL is
# stable (…/releases/latest/download/ironmud-x86_64-linux.tar.gz). The
# version is recorded in the VERSION file inside the tarball.
#
# Tarball layout (mirrors /opt/ironmud):
#   ironmud-<arch>-linux/
#     bin/{ironmud,ironmud-admin,ironmud-import}
#     scripts/{commands,triggers,lib,data,commands.json}
#     assets/{banner.txt,matrix_avatar.png}
#     install.sh  ironmud-sync.sh  README.md  VERSION

set -euo pipefail

VERSION=""
TARGET=""
OUT="dist"

while [[ $# -gt 0 ]]; do
    case $1 in
        --version) VERSION="$2"; shift 2 ;;
        --target) TARGET="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -z "$VERSION" ]]; then
    VERSION="v$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
fi

# Arch label from the target triple, or from the host when building natively.
triple="${TARGET:-$(rustc -vV | sed -n 's/^host: //p')}"
case "$triple" in
    x86_64-*) ARCH="x86_64" ;;
    aarch64-*) ARCH="aarch64" ;;
    *) echo "Unsupported target: $triple" >&2; exit 1 ;;
esac

if [[ -n "$TARGET" ]]; then
    BIN_DIR="target/$TARGET/release"
else
    BIN_DIR="target/release"
fi

for bin in ironmud ironmud-admin ironmud-import; do
    if [[ ! -x "$BIN_DIR/$bin" ]]; then
        echo "Missing $BIN_DIR/$bin -- run cargo build --release first" >&2
        exit 1
    fi
done

NAME="ironmud-${ARCH}-linux"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
PKG="$STAGE/$NAME"

mkdir -p "$PKG/bin" "$PKG/scripts" "$PKG/assets"

install -m 755 "$BIN_DIR/ironmud" "$BIN_DIR/ironmud-admin" "$BIN_DIR/ironmud-import" "$PKG/bin/"
if command -v strip >/dev/null 2>&1 && [[ -z "$TARGET" || "$TARGET" == "$(rustc -vV | sed -n 's/^host: //p')" ]]; then
    strip "$PKG/bin/"*
fi

cp -r scripts/commands scripts/triggers scripts/lib scripts/data "$PKG/scripts/"
cp scripts/commands.json "$PKG/scripts/"
for f in banner.txt matrix_avatar.png; do
    [[ -f "assets/$f" ]] && cp "assets/$f" "$PKG/assets/"
done
install -m 755 install.sh ironmud-sync.sh "$PKG/"
cp README.md "$PKG/"
echo "$VERSION" > "$PKG/VERSION"

mkdir -p "$OUT"
TARBALL="$OUT/$NAME.tar.gz"
# Normalise modes: a checkout made under a strict umask (0077) would
# otherwise ship files the unprivileged server user can't read.
chmod -R u+rwX,go+rX,go-w "$PKG"
tar -C "$STAGE" --owner=0 --group=0 -czf "$TARBALL" "$NAME"
(cd "$OUT" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")

echo "Packaged $VERSION ($ARCH): $TARBALL"
cat "$TARBALL.sha256"
