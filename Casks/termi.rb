cask "termi" do
  version "0.1.23"
  sha256 "ab74e6bc7f6d144614b0f1c5b9e9c20de2000e65657ed51d961068f298cd09b6"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
