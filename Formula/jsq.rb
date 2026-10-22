class Jsq < Formula
  desc "JavaSquirt: JavaScript semantics, Rust speed"
  homepage "https://github.com/00000kkkkk/javasquirt"
  version "1.3.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.3.0/javasquirt-v1.3.0-aarch64-apple-darwin.tar.gz"
      sha256 "e169b14403819ff815c7ec52e456acd1b3dc087577e4efd66527dbb0d6b02763"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.3.0/javasquirt-v1.3.0-x86_64-apple-darwin.tar.gz"
      sha256 "45628f112f5c5ff6d4fc7afd62a4045a23f2fde36e87acd00ff8a7ed92b0fa8c"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.3.0/javasquirt-v1.3.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "7633c70ac8c1cd01c1fba42960bca13d0f7b4b6e17ccd913f1af25939968ddbb"
    end
    on_intel do
      url "https://github.com/00000kkkkk/javasquirt/releases/download/v1.3.0/javasquirt-v1.3.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "b149032adc58df34fa58822dcedb6721c860b70f4ca456032246d50dc8433b7e"
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
