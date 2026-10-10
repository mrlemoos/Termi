cask "termi" do
  version "0.1.17"
  sha256 "a0426c109a077328c8146175b401fc62815c1feb346a70260855dde7a81d1342"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
