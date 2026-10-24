class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.10.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.10.0/javasquirt-v1.10.0-aarch64-apple-darwin.tar.gz"
      sha256 "f09a4a0773bb0728b0cda26b9e9750bc0e2184f9fd015ca578db1a2d24da102e"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.10.0/javasquirt-v1.10.0-x86_64-apple-darwin.tar.gz"
      sha256 "837893c8c743ea871794b9ce1b26a1804de86ba0d6fc23f14a7fe51bf0af1c1d"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.10.0/javasquirt-v1.10.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "dc67aa0038f756af61098bede6619270f27fc9319891b546b81baea469bfbb5c"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.10.0/javasquirt-v1.10.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "3984369fd60d500859bb5bb1453d02f9b942088e08a5757fca589c5c340634ea"
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
