#!/usr/bin/env bash
# Push current repo using GitHub PAT from ../token (repo-local, not committed).
set -euo pipefail
cd "$(dirname "$0")/.."
TOKEN_FILE="../token"
if [ ! -f "$TOKEN_FILE" ]; then
  echo "missing $TOKEN_FILE" >&2
  exit 1
fi
TOKEN=$(grep -E '^github_pat_' "$TOKEN_FILE" | tail -1)
if [ -z "$TOKEN" ]; then
  echo "no github_pat_ token found in $TOKEN_FILE" >&2
  exit 1
fi
REMOTE=$(git remote get-url origin)
# https://github.com/USER/REPO.git -> inject token
AUTH_URL=$(echo "$REMOTE" | sed -E "s|https://github.com/|https://x-access-token:${TOKEN}@github.com/|")
git push "$AUTH_URL" HEAD:main
echo "PUSH_OK $(basename "$(pwd)")"
