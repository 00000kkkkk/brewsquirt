#!/bin/sh
set -eu

REPO="00000kkkkk/javasquirt"
INSTALL_DIR="${JSQ_INSTALL_DIR:-$HOME/.jsq/bin}"
VERSION="${JSQ_VERSION:-}"

say() {
  printf '%s\n' "$*" >&2
}

fail() {
  say "install.sh: $*"
  exit 1
}

have() {
  command -v "$1" >/dev/null 2>&1
}

fetch() {
  if have curl; then
    curl -fsSL "$1"
  elif have wget; then
    wget -qO- "$1"
  else
    fail "curl or wget is required"
  fi
}

download() {
  if have curl; then
    curl -fsSL -o "$2" "$1"
  else
    wget -qO "$2" "$1"
  fi
}

sha256_of() {
  if have sha256sum; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif have shasum; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  elif have openssl; then
    openssl dgst -sha256 "$1" | sed 's/.*= *//'
  else
    say "no sha256 tool found, skipping checksum verification"
    cut -d ' ' -f 1 "$2"
  fi
}

detect_target() {
  os=$(uname -s)
  cpu=$(uname -m)
  case "$os" in
    Linux) sys="unknown-linux-gnu" ;;
    Darwin) sys="apple-darwin" ;;
    MINGW* | MSYS* | CYGWIN*) fail "on Windows run: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex" ;;
    *) fail "unsupported operating system: $os" ;;
  esac
  case "$cpu" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) fail "unsupported CPU: $cpu" ;;
  esac
  TARGET="$arch-$sys"
}

latest_version() {
  fetch "https://api.github.com/repos/$REPO/releases/latest" \
    | tr -d '\n' \
    | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/'
}

add_to_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) return ;;
  esac
  if [ -n "${JSQ_NO_MODIFY_PATH:-}" ]; then
    say "add $INSTALL_DIR to your PATH to run jsq from anywhere"
    return
  fi
  line="export PATH=\"$INSTALL_DIR:\$PATH\""
  updated=""
  for rc in "$HOME/.profile" "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [ -f "$rc" ] && ! grep -Fq "$INSTALL_DIR" "$rc"; then
      printf '\n%s\n' "$line" >> "$rc"
      updated="$updated $rc"
    fi
  done
  if [ -d "$HOME/.config/fish" ]; then
    fish_rc="$HOME/.config/fish/config.fish"
    if ! grep -Fqs "$INSTALL_DIR" "$fish_rc"; then
      printf '\nfish_add_path %s\n' "$INSTALL_DIR" >> "$fish_rc"
      updated="$updated $fish_rc"
    fi
  fi
  if [ -n "$updated" ]; then
    say "added $INSTALL_DIR to PATH in:$updated"
    say "open a new shell or run: $line"
  else
    say "add $INSTALL_DIR to your PATH to run jsq from anywhere"
  fi
}

main() {
  detect_target
  if [ -z "$VERSION" ]; then
    VERSION=$(latest_version)
    [ -n "$VERSION" ] || fail "could not determine the latest release"
  fi
  case "$VERSION" in
    v*) ;;
    *) VERSION="v$VERSION" ;;
  esac
  name="brewsquirt-$VERSION-$TARGET"
  base="https://github.com/$REPO/releases/download/$VERSION"
  tmp=$(mktemp -d 2>/dev/null || mktemp -d -t jsq)
  trap 'rm -rf "$tmp"' EXIT

  say "downloading BrewSquirt $VERSION for $TARGET"
  if ! download "$base/$name.tar.gz" "$tmp/$name.tar.gz" 2>/dev/null; then
    name="javasquirt-$VERSION-$TARGET"
    download "$base/$name.tar.gz" "$tmp/$name.tar.gz"
  fi
  download "$base/$name.sha256" "$tmp/$name.sha256"

  expected=$(cut -d ' ' -f 1 "$tmp/$name.sha256")
  actual=$(sha256_of "$tmp/$name.tar.gz" "$tmp/$name.sha256")
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $name.tar.gz"

  tar -xzf "$tmp/$name.tar.gz" -C "$tmp" jsq vrotdaem
  mkdir -p "$INSTALL_DIR"
  mv "$tmp/jsq" "$tmp/vrotdaem" "$INSTALL_DIR/"
  chmod +x "$INSTALL_DIR/jsq" "$INSTALL_DIR/vrotdaem"

  say "installed jsq and vrotdaem $VERSION to $INSTALL_DIR"
  add_to_path
  say "run 'jsq' to start the REPL or 'jsq app.jsq' to run a file"
}

main
