# Optional: a faster linker shortens every rebuild. Uncomment the one for your platform.
#
# Linux (lld):
# [target.x86_64-unknown-linux-gnu]
# linker = "clang"
# rustflags = ["-C", "link-arg=-fuse-ld=lld"]
#
# macOS (lld via Homebrew):
# [target.aarch64-apple-darwin]
# rustflags = ["-C", "link-arg=-fuse-ld=/opt/homebrew/bin/ld64.lld"]
#
# The dev-profile opt-levels (your code light, dependencies heavy) are in Cargo.toml.

[env]
# Look for assets next to the manifest even when the binary is run from elsewhere.
BEVY_ASSET_ROOT = { value = ".", relative = true }
