class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.9.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.9.0/javasquirt-v1.9.0-aarch64-apple-darwin.tar.gz"
      sha256 "e0e9b5d7704267ce89ded1837ba1c8a63bf00347333ec163eb484209fd79b24a"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.9.0/javasquirt-v1.9.0-x86_64-apple-darwin.tar.gz"
      sha256 "59a30d5b57f107c28bb11768706399233cd5c5fa8666f9a715c9aaaedcf9bf05"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.9.0/javasquirt-v1.9.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "784b7c3d27787fea6bb842f9eeb05dce11ccc54f637d259f780563b922e2c158"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.9.0/javasquirt-v1.9.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e3a2895201a3c9a5bafec770861e1d7847f7f900a632b41c4c7ec8c966141904"
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
