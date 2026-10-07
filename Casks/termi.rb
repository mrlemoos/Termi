cask "termi" do
  version "0.1.1"
  sha256 "05eafe443b7f62c7c378014919431e11735f2a6cbaaa4f93905c2c4fa515d7a4"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"
  depends_on arch: :arm64

  app "Termi.app"
end
