cask "termi" do
  version "0.1.19"
  sha256 "9f227b222b69686e23984ef143a3d46b50297cccbc7a388524bb7cfd35ba7b09"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
