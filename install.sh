#!/bin/sh
# Wake installer - https://github.com/joemckenney/wake
# Usage: curl -sSf https://raw.githubusercontent.com/joemckenney/wake/main/install.sh | sh

set -e

REPO="joemckenney/wake"
INSTALL_DIR="${WAKE_INSTALL_DIR:-$HOME/.local/bin}"

# Colors (if terminal supports it)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    NC='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    NC=''
fi

info() { printf "${GREEN}info${NC}: %s\n" "$1"; }
warn() { printf "${YELLOW}warn${NC}: %s\n" "$1"; }
error() { printf "${RED}error${NC}: %s\n" "$1" >&2; exit 1; }

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "darwin" ;;
        *)       error "Unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)  echo "x86_64" ;;
        aarch64|arm64) echo "aarch64" ;;
        *)             error "Unsupported architecture: $(uname -m)" ;;
    esac
}

detect_glibc_version() {
    # Returns glibc version as a comparable number (e.g., 2.39 -> 239)
    if command -v ldd >/dev/null 2>&1; then
        version=$(ldd --version 2>&1 | head -1 | grep -oE '[0-9]+\.[0-9]+' | head -1)
        if [ -n "$version" ]; then
            major=$(echo "$version" | cut -d. -f1)
            minor=$(echo "$version" | cut -d. -f2)
            echo $((major * 100 + minor))
            return
        fi
    fi
    echo "0"
}

get_target() {
    os="$1"
    arch="$2"

    case "${os}-${arch}" in
        linux-x86_64)
            # Allow override via WAKE_TARGET env var
            if [ "${WAKE_TARGET:-}" = "musl" ]; then
                echo "x86_64-unknown-linux-musl"
            elif [ "${WAKE_TARGET:-}" = "gnu" ]; then
                echo "x86_64-unknown-linux-gnu"
            else
                # Auto-detect: use musl if glibc < 2.39
                glibc_ver=$(detect_glibc_version)
                if [ "$glibc_ver" -lt 239 ] && [ "$glibc_ver" -gt 0 ]; then
                    warn "Detected glibc < 2.39, using static musl build"
                    echo "x86_64-unknown-linux-musl"
                else
                    echo "x86_64-unknown-linux-gnu"
                fi
            fi
            ;;
        darwin-x86_64)  echo "x86_64-apple-darwin" ;;
        darwin-aarch64) echo "aarch64-apple-darwin" ;;
        *)              error "Unsupported platform: ${os}-${arch}" ;;
    esac
}

get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

download() {
    url="$1"
    output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -sSfL "$url" -o "$output"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$output"
    fi
}

main() {
    os=$(detect_os)
    arch=$(detect_arch)
    target=$(get_target "$os" "$arch")

    info "Detected platform: ${os}-${arch} (${target})"

    # Get latest version
    info "Fetching latest release..."
    version=$(get_latest_version)

    if [ -z "$version" ]; then
        error "Failed to determine latest version. Check https://github.com/${REPO}/releases"
    fi

    info "Latest version: ${version}"

    # Create temp directory
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Download archive
    archive_name="wake-${version}-${target}.tar.gz"
    download_url="https://github.com/${REPO}/releases/download/${version}/${archive_name}"

    info "Downloading ${archive_name}..."
    download "$download_url" "${tmp_dir}/${archive_name}" || error "Failed to download from ${download_url}"

    # Extract
    info "Extracting..."
    tar -xzf "${tmp_dir}/${archive_name}" -C "$tmp_dir"

    # Find extracted directory
    extract_dir="${tmp_dir}/wake-${version}-${target}"

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Install binaries
    for bin in wake wake-mcp; do
        if [ -f "${extract_dir}/${bin}" ]; then
            mv "${extract_dir}/${bin}" "${INSTALL_DIR}/${bin}"
            chmod +x "${INSTALL_DIR}/${bin}"
            info "Installed ${bin} to ${INSTALL_DIR}/${bin}"
        else
            warn "Binary ${bin} not found in release"
        fi
    done

    # Check if install dir is in PATH
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            warn "${INSTALL_DIR} is not in your PATH"
            echo ""
            echo "Add this to your shell profile:"
            echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
            echo ""
            ;;
    esac

    info "Installation complete! Run 'wake --help' to get started."
}

main "$@"
