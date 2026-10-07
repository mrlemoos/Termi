cask "termi" do
  version "0.1.9"
  sha256 "58e43579144c98f1f7d290c4d6574d50a7037e75be1473c4f9ad0c4617c851fc"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
