# Copy to Formula/jlv.rb in the piterx93/homebrew-tap repository.
class Jlv < Formula
  desc "Terminal log viewer for Java application logs"
  homepage "https://github.com/piterx93/jlv"
  url "https://github.com/piterx93/jlv/archive/refs/tags/v1.0.0.tar.gz"
  # TODO: replace after the tag exists: curl -sL <url> | shasum -a 256
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/piterx93/jlv.git", branch: "develop"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/jlv --version")
  end
end
