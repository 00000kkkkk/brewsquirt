class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.8.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.8.0/javasquirt-v1.8.0-aarch64-apple-darwin.tar.gz"
      sha256 "3d6c72a929d1ac34704a027fa2ba7641341222bddaddd28ebc90239cddfca8a7"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.8.0/javasquirt-v1.8.0-x86_64-apple-darwin.tar.gz"
      sha256 "eed23e29de152a24a9dd52676034f1e7d4823c3f950952c7afcc1c4ce73e6891"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.8.0/javasquirt-v1.8.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "c089267ac69e5cd3aafc292d035902527773318765185acd5ad28cdc0b78f49f"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.8.0/javasquirt-v1.8.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "bd823c6b0dd8ffbcbf55b956c3123c995b0e59cba99bfb011b313d25ec95a6ac"
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
