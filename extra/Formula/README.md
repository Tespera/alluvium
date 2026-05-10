# `extra/Formula/` — Homebrew tap formula

This directory holds `alluvium.rb`, the Homebrew formula for installing
Alluvium via `brew`.

## Status (v0.1)

The formula exists but the **tap is not yet published**. To make
`brew tap Tespera/alluvium && brew install alluvium` actually work, the
project owner needs to:

1. Create a public GitHub repo `Tespera/homebrew-alluvium`.
2. Copy `alluvium.rb` from here into that repo's `Formula/` directory.
3. Cut a GitHub release on `Tespera/alluvium` (e.g. tag `v0.1.0`) with
   prebuilt tarballs as release assets:
   - `alluvium-v0.1.0-aarch64-apple-darwin.tar.gz`
   - `alluvium-v0.1.0-x86_64-apple-darwin.tar.gz`
   - `alluvium-v0.1.0-x86_64-unknown-linux-gnu.tar.gz`
4. Compute `sha256` for each and replace the `REPLACE_WITH_SHA256_*`
   placeholders in `alluvium.rb`.
5. Push to the tap repo.

These steps are project-owner work — the formula file in this directory
is the part that lives in the main repo so it stays version-locked with
the source.

## Building the release tarballs

```sh
# darwin-arm64
cargo build --release --target aarch64-apple-darwin
tar -czf alluvium-v0.1.0-aarch64-apple-darwin.tar.gz \
  -C target/aarch64-apple-darwin/release alluvium

# darwin-amd64
cargo build --release --target x86_64-apple-darwin
tar -czf alluvium-v0.1.0-x86_64-apple-darwin.tar.gz \
  -C target/x86_64-apple-darwin/release alluvium

# linux-amd64
cargo build --release --target x86_64-unknown-linux-gnu
tar -czf alluvium-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  -C target/x86_64-unknown-linux-gnu/release alluvium

# Then for each .tar.gz:
shasum -a 256 alluvium-v0.1.0-*.tar.gz
```

Paste the resulting hashes into `alluvium.rb`.
