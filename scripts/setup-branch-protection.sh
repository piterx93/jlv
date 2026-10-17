#!/usr/bin/env bash
# Applies scripts/branch-ruleset.json to main and develop: PRs only, ci-required
# must pass, no force pushes or deletion.
set -euo pipefail

repo="${1:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
gh api --method POST "repos/$repo/rulesets" --input "$(dirname "$0")/branch-ruleset.json"
