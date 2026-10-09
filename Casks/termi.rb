cask "termi" do
  version "0.1.13"
  sha256 "d5d0a5610a3ae48411e9e7d189466dd5428085da932585dac81eac86fdd57da1"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
