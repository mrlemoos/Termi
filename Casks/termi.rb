cask "termi" do
  version "0.1.12"
  sha256 "ad20dc2ba3257f76bb72de09ef348a7f02e095ce2e4f788a6462238899c27072"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
