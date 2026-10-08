#!/usr/bin/env bash
# Applies scripts/branch-ruleset.json to main and develop: PRs only, ci-required
# must pass, no force pushes or deletion.
#
# Disabled until the repo goes public at 1.0. Uncomment the two lines below then.
set -euo pipefail

# repo="${1:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
# gh api --method POST "repos/$repo/rulesets" --input "$(dirname "$0")/branch-ruleset.json"

echo "branch protection is disabled for now, see $0" >&2
