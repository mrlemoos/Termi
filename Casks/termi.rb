cask "termi" do
  version "0.1.27"
  sha256 "c13e8be4eabf39ceeb647a21936cc01d6d9ac1a4a82560f83134e50e2d159fb6"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
