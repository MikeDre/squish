#!/usr/bin/env bash
# Update the Homebrew tap formula for a squish release.
#
# Fetches the macOS artifact checksums from the GitHub release and regenerates
# Formula/squish.rb in the tap checkout, then commits and pushes. Idempotent:
# exits cleanly when the formula is already current.
#
# Usage: scripts/release-tap.sh vX.Y.Z [path-to-tap-checkout]
# Requires: gh (authenticated), a clone of MikeDre/homebrew-tap.
#
# Run automatically by the `tap` job in .github/workflows/release.yml after every
# tagged build. Safe to run by hand as well — it is idempotent, so re-running it
# to recover from a failed CI run costs nothing.
set -euo pipefail

tag="${1:?usage: release-tap.sh vX.Y.Z [tap-dir]}"
tap_dir="${2:-$(dirname "$0")/../../homebrew-tap}"
version="${tag#v}"
repo="MikeDre/squish"

if [ ! -d "$tap_dir/Formula" ]; then
  echo "error: tap checkout not found at $tap_dir (clone MikeDre/homebrew-tap first)" >&2
  exit 1
fi

# A tag that isn't vX.Y.Z would produce a formula whose `version` no longer
# matches what `squish --version` prints, and the formula's own `test do` block
# would fail for every user installing it.
#
# Fully anchored, and restricted to characters that are inert in Ruby: the tag is
# interpolated into the formula through a heredoc below, so a tag containing a
# quote or a `#{}` could otherwise rewrite the formula it is meant to describe.
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.]+)?$'; then
  echo "error: '$tag' is not a vX.Y.Z release tag" >&2
  exit 1
fi

echo "Fetching checksums for $tag..."
arm_sha=$(gh release download "$tag" --repo "$repo" \
  --pattern "*aarch64-apple-darwin.tar.gz.sha256" --output - | awk '{print $1}')
x64_sha=$(gh release download "$tag" --repo "$repo" \
  --pattern "*x86_64-apple-darwin.tar.gz.sha256" --output - | awk '{print $1}')

# A malformed checksum here is worse than no release at all: Homebrew would
# reject the download for every user, and the formula would look correct.
for pair in "arm64:$arm_sha" "x86_64:$x64_sha"; do
  arch="${pair%%:*}" sha="${pair#*:}"
  if ! printf '%s' "$sha" | grep -Eq '^[0-9a-f]{64}$'; then
    echo "error: $arch checksum from the $tag release is not a sha256: '$sha'" >&2
    exit 1
  fi
done

cat > "$tap_dir/Formula/squish.rb" << EOF
class Squish < Formula
  desc "Super fast local file optimisation: images, video, audio, and code"
  homepage "https://github.com/MikeDre/squish"
  version "$version"
  license "MIT"

  # dav1d and libheif are linked at load time (HEIC/AVIF support); ffmpeg and
  # gifsicle are runtime subprocess dependencies for video/audio and GIF.
  depends_on "dav1d"
  depends_on "ffmpeg"
  depends_on "gifsicle"
  depends_on "libheif"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/MikeDre/squish/releases/download/$tag/squish-$tag-aarch64-apple-darwin.tar.gz"
      sha256 "$arm_sha"
    else
      url "https://github.com/MikeDre/squish/releases/download/$tag/squish-$tag-x86_64-apple-darwin.tar.gz"
      sha256 "$x64_sha"
    end
  end

  def install
    bin.install "squish"
    man1.install "squish.1"
    generate_completions_from_executable(bin/"squish", "completions")
  end

  def caveats
    <<~EOS
      To add the "Right-click → Squish" Finder Quick Action, run:
        squish finder-action install
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/squish --version")
  end
end
EOF

cd "$tap_dir"
if git diff --quiet Formula/squish.rb; then
  echo "Formula already up to date for $tag — nothing to do."
  exit 0
fi

# CI checkouts carry no identity. Fall back to the account that owns the tap so
# its history stays consistently authored whether this runs locally or in Actions;
# a configured identity always wins.
if ! git config user.email >/dev/null; then
  git config user.name "MikeAndré"
  git config user.email "info@mikedre.com"
fi

git add Formula/squish.rb
git commit -m "🔧 Updated: squish formula to $version [main]"
git push
echo "Tap updated to squish $version."
