cask "termi" do
  version "0.1.16"
  sha256 "44e3aa495def45972769bad23ce08a44570ef68ffe5ceecc20aa8fae452d839f"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
