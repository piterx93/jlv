# Copy to Formula/jlv.rb in the piterx93/homebrew-tap repository.
class Jlv < Formula
  desc "Terminal log viewer for Java application logs"
  homepage "https://github.com/piterx93/jlv"
  url "https://github.com/piterx93/jlv/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "b87683f3926c97c189f14106cc03ea6c7a303dbcb3afae4771a609c6d61c720b"
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
