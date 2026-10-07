cask "termi" do
  version "0.1.10"
  sha256 "58484f39a4b833f1ffe01a2bb0e07477b0909fc78d14e0b64a55d70f548bb522"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
