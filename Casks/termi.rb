cask "termi" do
  version "0.1.7"
  sha256 "6aec6ce256893708a6c1abe88a6d6879aada82253a4b1140a1a00a53b4ab206b"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
