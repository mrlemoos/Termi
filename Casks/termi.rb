cask "termi" do
  version "0.1.6"
  sha256 "ac5e459a715f8da2bebf46fb545b479926d542c42b32c420905bbca04237e276"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"
  depends_on arch: :arm64

  app "Termi.app"
end
