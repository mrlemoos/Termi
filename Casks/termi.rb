cask "termi" do
  version "0.1.24"
  sha256 "93c67f2a9d3aac79af1a3724f881e9d381fa9f70e1df5012f8b96f6ed8484e23"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
