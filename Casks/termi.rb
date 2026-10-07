cask "termi" do
  version "0.1.11"
  sha256 "63e45d5ee4039c8863d4c0655aaef7a5369be9e6008aa24b0bcee39e1db71fa9"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
