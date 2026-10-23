class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.7.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.7.0/javasquirt-v1.7.0-aarch64-apple-darwin.tar.gz"
      sha256 "00fc7ea21422ff3ef9868360a868b560f6aad85ee80e3d2651fd38b480ce5374"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.7.0/javasquirt-v1.7.0-x86_64-apple-darwin.tar.gz"
      sha256 "6da1cc6ac87ab5190d6a7bca1b68332279addc3f747bdb61ff27dfa5678b58c6"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.7.0/javasquirt-v1.7.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "b271d0005742057cd90937933c20463ae1bb9c7982631a07b601a2521fc1d1f3"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.7.0/javasquirt-v1.7.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0046976c615c4dfe4f65b6930ba8e7bc3c94f9b354040b2b1ad64f892c53a81f"
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
