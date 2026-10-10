cask "termi" do
  version "0.1.18"
  sha256 "079b20e75137c41f05e103c307f003146d73466ae51151aae8db19bbb2be96f2"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
