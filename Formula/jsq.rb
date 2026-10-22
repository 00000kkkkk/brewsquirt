class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.1.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.1.0/javasquirt-v1.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "0a4590941aa004b48320e522084092e672ebd4757ebfef8c438bec6de5549941"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.1.0/javasquirt-v1.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "189040a69a55de4d036fae76dfc167d9d96c44087a1c713b0c7c32cd123a28bd"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.1.0/javasquirt-v1.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "a0a144bb0d451b554bb6aeb56a06723ee699b2e60122aa42dc5dadd6f3e0a7d6"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.1.0/javasquirt-v1.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "5da24c9727c68c397876a228213c7db8e5fc34fec115a6ff1e5e9978c04ee666"
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
