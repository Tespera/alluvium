# Homebrew formula for Alluvium.
#
# This file is a TEMPLATE. To ship it as a real `brew install`, you (the
# project owner) must:
#
#   1. Create a public GitHub repo named `homebrew-alluvium`.
#   2. Copy this file into that repo as `Formula/alluvium.rb`.
#   3. Cut a GitHub release on the main `alluvium` repo (e.g. tag v0.1.0)
#      and upload the prebuilt darwin-arm64 / darwin-amd64 / linux-amd64
#      tarballs as release assets — naming pattern:
#        alluvium-v<version>-<target>.tar.gz
#      where <target> is one of:
#        aarch64-apple-darwin / x86_64-apple-darwin / x86_64-unknown-linux-gnu
#   4. Compute sha256 for each tarball and replace the `REPLACE_WITH_SHA256`
#      placeholders below.
#   5. Push the formula. Users then run:
#        brew tap Tespera/alluvium
#        brew install alluvium
#
# Until those steps happen, users install via:
#   cargo install --path .             # from a checkout
#   cargo install --git https://github.com/Tespera/alluvium  # remote
#
# The Cargo.toml's [package] section is the source of truth for `version`
# and `description`; this file mirrors what we publish on each release.

class Alluvium < Formula
  desc "Auto-archive Claude Code sessions to your Obsidian vault as a Karpathy-style LLM wiki"
  homepage "https://github.com/Tespera/alluvium"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/Tespera/alluvium/releases/download/v#{version}/alluvium-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_DARWIN_ARM64"
    end
    on_intel do
      url "https://github.com/Tespera/alluvium/releases/download/v#{version}/alluvium-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_DARWIN_AMD64"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Tespera/alluvium/releases/download/v#{version}/alluvium-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_LINUX_AMD64"
    end
  end

  def install
    bin.install "alluvium"
  end

  def caveats
    <<~EOS
      Alluvium is a Claude Code Plugin. After install:

        1. Run `alluvium init` to configure your vault path + LLM backend.
        2. Run `claude plugin install $(alluvium plugin-path)` to register
           the 4 hooks with Claude Code.
        3. From then on, every Claude Code session you finish will be
           distilled into your vault automatically.

      Docs: https://github.com/Tespera/alluvium#readme
    EOS
  end

  test do
    # Smoke test: the binary runs and reports its version.
    assert_match version.to_s, shell_output("#{bin}/alluvium --version")
  end
end
