class Jsq < Formula
  desc "BrewSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/brewsquirt"
  version "1.11.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/brewsquirt/releases/download/v1.11.0/brewsquirt-v1.11.0-aarch64-apple-darwin.tar.gz"
      sha256 "fc7a627e28d899e16484c77edcbca08ccce58f1734179afe4ae0594529d21e70"
    end
    on_intel do
      url "https://github.com/00000kkkkk/brewsquirt/releases/download/v1.11.0/brewsquirt-v1.11.0-x86_64-apple-darwin.tar.gz"
      sha256 "446e842fb26637bfa191cdeb6f8ce1741a2f42a9b4abccd43c3422981c685a68"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/brewsquirt/releases/download/v1.11.0/brewsquirt-v1.11.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "b28ec17051ae6fc4aba9300de1f5ca110deb9bee8029f1be032a7578391e65a1"
    end
    on_intel do
      url "https://github.com/00000kkkkk/brewsquirt/releases/download/v1.11.0/brewsquirt-v1.11.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "cc689682d349deb312538e50c247096c6b59cdb95ddab6961ea833c2a1f40cdf"
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
