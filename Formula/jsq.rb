class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.4.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.4.0/javasquirt-v1.4.0-aarch64-apple-darwin.tar.gz"
      sha256 "23672be1b702d8abb223c395fc30f2ba620605d238922acea6aa66c4dbcfb4ae"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.4.0/javasquirt-v1.4.0-x86_64-apple-darwin.tar.gz"
      sha256 "247f5e9d504694e0ece82e5e76b496efb4398b7630bc977d1e8a3a0fe6ea9915"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.4.0/javasquirt-v1.4.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "26896a653d77196d2692d04c14a54f34c29c5a41ad7ae8a66e51c9d5135bee19"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.4.0/javasquirt-v1.4.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e8fe885c8d8697cc41190d5981106e46501e21218fb45c7c6c0daefdfd00a305"
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
