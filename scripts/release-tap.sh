#!/usr/bin/env bash
# Update the Homebrew tap formula for a squish release.
#
# Fetches the macOS artifact checksums from the GitHub release and regenerates
# Formula/squish.rb in the tap checkout, then commits and pushes. Idempotent:
# exits cleanly when the formula is already current.
#
# Usage: scripts/release-tap.sh vX.Y.Z [path-to-tap-checkout]
# Requires: gh (authenticated), a clone of MikeDre/homebrew-tap.
set -euo pipefail

tag="${1:?usage: release-tap.sh vX.Y.Z [tap-dir]}"
tap_dir="${2:-$(dirname "$0")/../../homebrew-tap}"
version="${tag#v}"
repo="MikeDre/squish"

if [ ! -d "$tap_dir/Formula" ]; then
  echo "error: tap checkout not found at $tap_dir (clone MikeDre/homebrew-tap first)" >&2
  exit 1
fi

echo "Fetching checksums for $tag..."
arm_sha=$(gh release download "$tag" --repo "$repo" \
  --pattern "*aarch64-apple-darwin.tar.gz.sha256" --output - | awk '{print $1}')
x64_sha=$(gh release download "$tag" --repo "$repo" \
  --pattern "*x86_64-apple-darwin.tar.gz.sha256" --output - | awk '{print $1}')

if [ -z "$arm_sha" ] || [ -z "$x64_sha" ]; then
  echo "error: could not fetch both macOS checksums from the $tag release" >&2
  exit 1
fi

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

git add Formula/squish.rb
git commit -m "🔧 Updated: squish formula to $version [main]"
git push
echo "Tap updated to squish $version."
