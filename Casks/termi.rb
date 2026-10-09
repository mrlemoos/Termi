cask "termi" do
  version "0.1.14"
  sha256 "4f197fd4ddcaf05db367b6395d5c2ad7a3d816e90759621b7a52b7867aad5691"

  url "https://github.com/mrlemoos/Termi/releases/download/v#{version}/Termi.zip"
  name "Termi"
  desc "Chromeless terminal for coding agents"
  homepage "https://github.com/mrlemoos/Termi"

  depends_on macos: :big_sur
  depends_on arch: :arm64

  app "Termi.app"
end
