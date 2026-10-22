class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.2.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.2.0/javasquirt-v1.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "3523f7f77b5df6e9e496a2e0ec83f65605813cec1383b430e42765ed33b0fcff"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.2.0/javasquirt-v1.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "f03fb558d3de49d8c6929f7b1e9eb1491521ade309acaec282dd017aecbb5beb"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.2.0/javasquirt-v1.2.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "e88f13951006897b97bb62147b03da05d433a6fa54fa03bddf1287f06cd3b9a9"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.2.0/javasquirt-v1.2.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "4edd111e9b4d7358a28ad335e67ddafd2b3ee61bc02d90670e7ef97116081c1c"
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
