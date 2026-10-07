cask "termi" do
  version "0.1.4"
  sha256 "98a7ddec6fd9796256e415844157efc135f24b2586a9f1f633aa55644c1c1626"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"
  depends_on arch: :arm64

  app "Termi.app"
end
