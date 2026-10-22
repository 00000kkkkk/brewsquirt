class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.0.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.0.0/javasquirt-v1.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "8ee0c2fbfd2294115592f2462f65f0147df276290eef5d6b16fb24cd4f42f7cd"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.0.0/javasquirt-v1.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "e1362cbcebc7d943933954b8de865239c8c35b942c8248b015937af986214b8a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.0.0/javasquirt-v1.0.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "1b32a0aa2502530cb123cc3e372232cc02cf6eb95b8dcfb1a21839892e2e8052"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.0.0/javasquirt-v1.0.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "224a937d8c8f3d5be55504bc665cfa875a0437569cd4d5f4902253b0cc47f685"
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
