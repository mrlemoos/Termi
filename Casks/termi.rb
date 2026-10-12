cask "termi" do
  version "0.1.20"
  sha256 "7347fe5d7ff3d9b56196d3083e5256389f4662639f40135057cd8d01fdb7f51b"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
