cask "termi" do
  version "0.1.2"
  sha256 "3d604dac207788f453686e9852627638c12a89592e4bafea6d246f88e3853475"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"
  depends_on arch: :arm64

  app "Termi.app"
end
