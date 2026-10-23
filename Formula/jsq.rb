class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.6.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.6.0/javasquirt-v1.6.0-aarch64-apple-darwin.tar.gz"
      sha256 "67954811dc8811fc5ca02b8c81e6263a6a16e0fc92a8bdad384647ab7f3b7af4"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.6.0/javasquirt-v1.6.0-x86_64-apple-darwin.tar.gz"
      sha256 "e1716471465e2cc30bea3dc40908d26843a0bc33155dc3ec63d3858945d654d2"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.6.0/javasquirt-v1.6.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "91da8b7e2b71adfee2f3ad7b322603eb0f0db454c6f89c9fd861ae722792d23d"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.6.0/javasquirt-v1.6.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "32d3196f91a5aa9a0b0bfcac00a180abb19034d3d0f9cc06dc1e7fecc7b3d51c"
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
