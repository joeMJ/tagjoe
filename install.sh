#!/usr/bin/env bash
# Installs or updates tagjoe from the newest GitHub release (.deb for Ubuntu 24.04, Ubuntu 26.04,
# Debian 12 and Debian 13, amd64).
# Call: curl -fsSL https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh | bash
# Everything is inside main(), so that bash does not run a half-read script with "curl | bash".
set -euo pipefail

main() {
    local repo="joeMJ/tagjoe"
    # shellcheck disable=SC1091
    . /etc/os-release
    local distro="${ID}${VERSION_ID}"
    case "$distro" in
        ubuntu24.04 | ubuntu26.04 | debian12 | debian13) ;;
        *) echo "✖ tagjoe is packaged for Ubuntu 24.04/26.04 and Debian 12/13 (this is: ${PRETTY_NAME:-$distro})." >&2
           exit 1 ;;
    esac
    local arch
    arch=$(dpkg --print-architecture)
    [ "$arch" = amd64 ] || { echo "✖ The tagjoe package is for amd64 only (this is: $arch)." >&2; exit 1; }

    # As root (Debian without sudo) directly, otherwise with sudo.
    local root=()
    [ "$(id -u)" = 0 ] || root=(sudo)

    local tmp
    tmp=$(mktemp -d)
    # The path goes into the trap now: at exit, main() and its local tmp are gone already.
    # shellcheck disable=SC2064
    trap "rm -rf '$tmp'" EXIT
    # apt reads local packages as the user _apt: make the folder and the file readable.
    chmod 755 "$tmp"

    echo "» Looking for the newest release for $distro/$arch …"
    local json
    json=$(curl -fsSL -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/$repo/releases/latest")
    local url sums
    url=$(printf '%s\n' "$json" | grep -oE "\"browser_download_url\": *\"[^\"]*/tagjoe_[^\"/]*_${arch}\\.deb\"" |
        sed -E 's/.*"(https[^"]*)"/\1/' | head -n1)
    sums=$(printf '%s\n' "$json" | grep -oE '"browser_download_url": *"[^"]*/SHA256SUMS"' |
        sed -E 's/.*"(https[^"]*)"/\1/' | head -n1)
    if [ -z "$url" ] || [ -z "$sums" ]; then
        echo "✖ The newest release has no package for $arch." >&2
        exit 1
    fi

    local deb
    deb="$tmp/$(basename "$url")"
    echo "» Downloading $(basename "$deb") …"
    curl -fsSL -o "$deb" "$url"
    curl -fsSL -o "$tmp/SHA256SUMS" "$sums"
    chmod 644 "$deb"
    echo "» Checking the checksum …"
    (cd "$tmp" && grep -E " \*?$(basename "$deb")\$" SHA256SUMS | sha256sum -c -)

    if [ ${#root[@]} = 0 ]; then echo "» Installing …"; else echo "» Installing (asks for the sudo password) …"; fi
    "${root[@]}" apt-get install -y "$deb"
    echo "✔ tagjoe is installed. Start it from the menu or with: tagjoe-gui (command line: tagjoe)"
    echo "  Every online source is off until you switch it on under Settings → Sources."
}

main "$@"
