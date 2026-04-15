#!/usr/bin/env bash
# squish — install system dependencies for development and runtime.
#
# This script is idempotent — safe to re-run. It detects your OS and uses
# the appropriate package manager.
#
# System deps:
#   - gifsicle   (GIF compression)
#   - libheif    (HEIC decode/encode)
#   - x265       (HEIC HEVC encoder)
#   - dav1d      (AVIF decoding)

set -euo pipefail

die() { echo "ERROR: $*" >&2; exit 1; }
info() { echo "==> $*"; }

detect_os() {
    case "$(uname -s)" in
        Darwin) echo "macos" ;;
        Linux)  echo "linux" ;;
        *)      die "unsupported OS: $(uname -s)" ;;
    esac
}

ensure_rust() {
    if ! command -v cargo >/dev/null 2>&1; then
        die "Rust not installed. Install from https://rustup.rs/ and re-run."
    fi
    info "Rust found: $(rustc --version)"
}

install_macos() {
    if ! command -v brew >/dev/null 2>&1; then
        die "Homebrew not installed. Install from https://brew.sh/ and re-run."
    fi
    info "Installing system deps via Homebrew..."
    brew install gifsicle libheif x265 dav1d pkg-config
}

install_linux() {
    if command -v apt-get >/dev/null 2>&1; then
        info "Installing system deps via apt..."
        sudo apt-get update
        sudo apt-get install -y \
            gifsicle \
            libheif-dev \
            libx265-dev \
            libdav1d-dev \
            pkg-config
    elif command -v dnf >/dev/null 2>&1; then
        info "Installing system deps via dnf..."
        sudo dnf install -y gifsicle libheif-devel x265-devel dav1d-devel pkgconfig
    elif command -v pacman >/dev/null 2>&1; then
        info "Installing system deps via pacman..."
        sudo pacman -S --needed --noconfirm gifsicle libheif x265 dav1d pkgconf
    else
        die "no supported package manager found (tried apt, dnf, pacman)"
    fi
}

main() {
    ensure_rust
    case "$(detect_os)" in
        macos) install_macos ;;
        linux) install_linux ;;
    esac

    info "Verifying installations..."
    command -v gifsicle >/dev/null && echo "  gifsicle: $(gifsicle --version | head -1)" \
        || echo "  WARN: gifsicle not found on PATH"
    command -v heif-enc >/dev/null && echo "  libheif: $(heif-enc --help 2>&1 | grep -o 'version [^ ]*' | head -1)" \
        || echo "  WARN: heif-enc not found on PATH"

    echo
    info "Done. Next step: cargo install --path crates/squish-cli"
}

main "$@"
