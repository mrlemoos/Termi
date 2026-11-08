cask "termi" do
  version "0.1.26"
  sha256 "d9f00fa6d20d4c9173097493c74f2f0eb30e019e57e0718e6b3b800fe61ed2d9"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
