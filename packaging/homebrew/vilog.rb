# Copy to Formula/vilog.rb in the piterx93/homebrew-tap repository, next to formula_renames.json
# mapping "jlv" to "vilog".
class Vilog < Formula
  desc "Terminal log viewer for application logs"
  homepage "https://github.com/piterx93/vilog"
  url "https://github.com/piterx93/vilog/archive/refs/tags/v2.0.0.tar.gz"
  sha256 "81b3f5fafd69fcb743f7aa381a2e91423059760742b5e75f52a25311d28bc1af"
  license any_of: ["MIT", "Apache-2.0"]
  head "https://github.com/piterx93/vilog.git", branch: "develop"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/vilog --version")
  end
end
