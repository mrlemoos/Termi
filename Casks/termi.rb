cask "termi" do
  version "0.1.28"
  sha256 "8cd8efce8828a275848bd468fa30426904af031a7bafe8f4696927bc31ffdf90"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
