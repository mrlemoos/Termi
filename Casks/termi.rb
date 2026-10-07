cask "termi" do
  version "0.1.8"
  sha256 "cb91e9153b61d56f2a3be7d7c8f2d6e45538d21dc5c2defa476933850414219f"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
