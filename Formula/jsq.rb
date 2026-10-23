class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.5.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.5.0/javasquirt-v1.5.0-aarch64-apple-darwin.tar.gz"
      sha256 "832d2bad064af1063f5a11aa81cb1306dfbc6ccc3192535ef46737be3ffc64d4"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.5.0/javasquirt-v1.5.0-x86_64-apple-darwin.tar.gz"
      sha256 "0f1c902654c4a9c25b257ddf42eeb9d015bcf221ca3d6d5ac89a54e2f348b2d2"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.5.0/javasquirt-v1.5.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0e0257effd2f26a8939119d451581a4ff440e0d6cc9da1dcad899600cba9ffa4"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.5.0/javasquirt-v1.5.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e2c62e43a6e18bf2dd1123077342c9fea06bb8635837af0cef4514e8d397d4bc"
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
