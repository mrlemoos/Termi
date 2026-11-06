cask "termi" do
  version "0.1.22"
  sha256 "368f656f33a86f82f776dab974893cb7769abd9583b03b294e36b9cd90c9b9a2"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
