#!/usr/bin/env bash
# Baut ein .deb (Vorbild: tvjoe/tools/build-deb.sh). Binärdateien sind an die glibc der
# Bauumgebung gebunden: auf Debian 12 gebaut laufen sie auf Debian 12/13 und Ubuntu 24.04/26.04.
# Ergebnis: target/deb/tagjoe_<version>_<distro>_<arch>.deb und target/deb/SHA256SUMS
set -euo pipefail
umask 022
dir="$(cd "$(dirname "$0")/.." && pwd)"
cd "$dir"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
. /etc/os-release
distro="${ID}${VERSION_ID}"
arch=$(dpkg --print-architecture)

cargo build --release --locked

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
chmod 755 "$stage"
install -Dm755 target/release/tagjoe "$stage/usr/bin/tagjoe"
install -Dm755 target/release/tagjoe-gui "$stage/usr/bin/tagjoe-gui"
install -Dm644 data/io.github.joeMJ.tagjoe.desktop "$stage/usr/share/applications/io.github.joeMJ.tagjoe.desktop"
install -Dm644 data/io.github.joeMJ.tagjoe.svg "$stage/usr/share/icons/hicolor/scalable/apps/io.github.joeMJ.tagjoe.svg"
for s in 48 64 128 256 512; do
    [ -f "data/icons/io.github.joeMJ.tagjoe-$s.png" ] && install -Dm644 "data/icons/io.github.joeMJ.tagjoe-$s.png" "$stage/usr/share/icons/hicolor/${s}x${s}/apps/io.github.joeMJ.tagjoe.png"
done
install -Dm644 LICENSE "$stage/usr/share/doc/tagjoe/copyright"
[ -f THIRD_PARTY.md ] && install -Dm644 THIRD_PARTY.md "$stage/usr/share/doc/tagjoe/THIRD_PARTY.md"
[ -f THIRD_PARTY_LICENSES.txt ] && install -Dm644 THIRD_PARTY_LICENSES.txt "$stage/usr/share/doc/tagjoe/THIRD_PARTY_LICENSES.txt"

size=$(du -sk "$stage/usr" | cut -f1)
mkdir -p "$stage/DEBIAN"
cat > "$stage/DEBIAN/control" <<CTRL
Package: tagjoe
Version: $version
Architecture: $arch
Maintainer: joeMJ <1265358+joeMJ@users.noreply.github.com>
Installed-Size: $size
Depends: libc6, libxkbcommon0, libxkbcommon-x11-0, libxcursor1, libxrandr2, libxi6, libwayland-client0, libgl1
Recommends: libchromaprint-tools, mesa-vulkan-drivers
Section: sound
Priority: optional
Homepage: https://github.com/joeMJ/tagjoe
Description: Check and fix audio tags, covers and file names
 FileBot-style tool for audio: drop a folder (album, compilation or
 audiobook), tagjoe fingerprints and identifies it (AcoustID, MusicBrainz,
 Cover Art Archive, Discogs, AudiobookDB, Audnexus), checks every tag field,
 proposes tags, cover and file names and applies them with an undo journal.
CTRL

out=target/deb
mkdir -p "$out"
deb="$out/tagjoe_${version}_${distro}_${arch}.deb"
dpkg-deb --root-owner-group --build "$stage" "$deb" >/dev/null
(cd "$out" && sha256sum tagjoe_*.deb > SHA256SUMS)
echo "$deb"
dpkg-deb --info "$deb" | sed -n '/Package:/,$p'
