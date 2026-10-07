cask "termi" do
  version "0.1.3"
  sha256 "a2bbbc32a43ede93e18628f0d469d670b155d8618d2f19869e4d12660866d74e"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"
  depends_on arch: :arm64

  app "Termi.app"
end
