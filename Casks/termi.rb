cask "termi" do
  version "0.1.0"
  sha256 "3457ba3b9d99ff3575459116b5960a541bafdb3eca8dffb0a431fe48697eb0e4"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: ">= :big_sur"

  app "Termi.app"
end
