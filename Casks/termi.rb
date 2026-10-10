cask "termi" do
  version "0.1.15"
  sha256 "cfadd2545136bc0073445ac80f478c64e9ea9b0ef4b629cc90bd2b0b81ae523c"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
