#!/bin/sh
set -eu

REPO="00000kkkkk/javasquirt"
TAG="${1:-}"
[ -n "$TAG" ] || {
  echo "usage: scripts/update-manifests.sh vX.Y.Z" >&2
  exit 1
}
case "$TAG" in
  v*) ;;
  *) TAG="v$TAG" ;;
esac
VERSION="${TAG#v}"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TMP=$(mktemp -d 2>/dev/null || mktemp -d -t jsq)
trap 'rm -rf "$TMP"' EXIT

gh release download "$TAG" -R "$REPO" -p '*.sha256' -D "$TMP"

hash_for() {
  cut -d ' ' -f 1 "$TMP/brewsquirt-$TAG-$1.sha256"
}

WIN=$(hash_for x86_64-pc-windows-msvc)
MAC_ARM=$(hash_for aarch64-apple-darwin)
MAC_X64=$(hash_for x86_64-apple-darwin)
LINUX_ARM=$(hash_for aarch64-unknown-linux-gnu)
LINUX_X64=$(hash_for x86_64-unknown-linux-gnu)
BASE="https://github.com/$REPO/releases/download/$TAG"

cat > "$ROOT/bucket/jsq.json" <<EOF
{
    "version": "$VERSION",
    "description": "BrewSquirt: JavaScript semantics, Rust speed. Installs the jsq runtime and the vrotdaem package manager.",
    "homepage": "https://github.com/$REPO",
    "license": "Apache-2.0",
    "architecture": {
        "64bit": {
            "url": "$BASE/brewsquirt-$TAG-x86_64-pc-windows-msvc.zip",
            "hash": "$WIN"
        }
    },
    "bin": [
        "jsq.exe",
        "vrotdaem.exe"
    ],
    "checkver": "github",
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/$REPO/releases/download/v\$version/brewsquirt-v\$version-x86_64-pc-windows-msvc.zip",
                "hash": {
                    "url": "https://github.com/$REPO/releases/download/v\$version/brewsquirt-v\$version-x86_64-pc-windows-msvc.sha256"
                }
            }
        }
    }
}
EOF

cat > "$ROOT/Formula/jsq.rb" <<EOF
class Jsq < Formula
  desc "BrewSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/$REPO"
  version "$VERSION"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "$BASE/brewsquirt-$TAG-aarch64-apple-darwin.tar.gz"
      sha256 "$MAC_ARM"
    end
    on_intel do
      url "$BASE/brewsquirt-$TAG-x86_64-apple-darwin.tar.gz"
      sha256 "$MAC_X64"
    end
  end

  on_linux do
    on_arm do
      url "$BASE/brewsquirt-$TAG-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$LINUX_ARM"
    end
    on_intel do
      url "$BASE/brewsquirt-$TAG-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$LINUX_X64"
    end
  end

  def install
    bin.install "jsq"
    bin.install "vrotdaem"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/jsq --version")
    assert_equal "1024", shell_output("#{bin}/jsq -e 'console.log(2 ** 10)'").strip
  end
end
EOF

echo "updated bucket/jsq.json and Formula/jsq.rb for $TAG"
