#!/bin/sh
# Show release statistics for wake
# Usage: ./scripts/release-stats.sh

set -e

REPO="joemckenney/wake"

if ! command -v gh >/dev/null 2>&1; then
    echo "Error: gh CLI is required. Install from https://cli.github.com/" >&2
    exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
    echo "Error: jq is required. Install with your package manager." >&2
    exit 1
fi

echo "Release Statistics for ${REPO}"
echo "================================"
echo ""

# Get all releases with download counts
releases=$(gh api "repos/${REPO}/releases" --paginate)

# Calculate totals
total_downloads=$(echo "$releases" | jq '[.[].assets[].download_count] | add // 0')
release_count=$(echo "$releases" | jq 'length')
latest_version=$(echo "$releases" | jq -r '.[0].tag_name')
latest_date=$(echo "$releases" | jq -r '.[0].published_at | split("T")[0]')

echo "Latest: ${latest_version} (${latest_date})"
echo "Total releases: ${release_count}"
echo "Total downloads: ${total_downloads}"
echo ""

# Per-release breakdown
echo "Downloads by Release"
echo "--------------------"
printf "%-12s %-12s %8s %8s %8s %8s\n" "VERSION" "DATE" "DARWIN-A" "DARWIN-X" "LINUX" "TOTAL"
echo ""

echo "$releases" | jq -r '
  .[] |
  .tag_name as $tag |
  (.published_at | split("T")[0]) as $date |
  (.assets | map(select(.name | contains("aarch64-apple"))) | .[0].download_count // 0) as $darwin_arm |
  (.assets | map(select(.name | contains("x86_64-apple"))) | .[0].download_count // 0) as $darwin_x86 |
  (.assets | map(select(.name | contains("linux-gnu"))) | .[0].download_count // 0) as $linux |
  ($darwin_arm + $darwin_x86 + $linux) as $total |
  "\($tag)|\($date)|\($darwin_arm)|\($darwin_x86)|\($linux)|\($total)"
' | while IFS='|' read -r tag date darwin_arm darwin_x86 linux total; do
    printf "%-12s %-12s %8s %8s %8s %8s\n" "$tag" "$date" "$darwin_arm" "$darwin_x86" "$linux" "$total"
done

echo ""
echo "Platform Totals"
echo "---------------"
echo "$releases" | jq -r '
  ([ .[].assets[] | select(.name | contains("aarch64-apple")) | .download_count ] | add // 0) as $darwin_arm |
  ([ .[].assets[] | select(.name | contains("x86_64-apple")) | .download_count ] | add // 0) as $darwin_x86 |
  ([ .[].assets[] | select(.name | contains("linux-gnu")) | .download_count ] | add // 0) as $linux |
  ([ .[].assets[] | select(.name | contains("linux-musl")) | .download_count ] | add // 0) as $musl |
  "macOS ARM64:  \($darwin_arm)\nmacOS x86_64: \($darwin_x86)\nLinux GNU:    \($linux)\nLinux musl:   \($musl)"
'
