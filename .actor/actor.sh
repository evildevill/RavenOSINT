#!/bin/bash
set -e

INPUT=$(cat)

USERNAMES=$(echo "$INPUT" | jq -r '.usernames[]' 2>/dev/null)

if [ -z "$USERNAMES" ]; then
  echo "No usernames provided"
  exit 1
fi

for username in $USERNAMES; do
  echo "Searching for: $username"

  OUTPUT=$(raven --no-color --csv /tmp/raven_output.csv "$username" 2>&1 || true)

  LINKS=$(tail -n +2 /tmp/raven_output.csv 2>/dev/null | grep ",Claimed," | cut -d',' -f3 || true)

  LINKS_JSON="[]"
  if [ -n "$LINKS" ]; then
    LINKS_JSON=$(echo "$LINKS" | jq -R -s -c 'split("\n") | map(select(length > 0))')
  fi

  RESULT=$(jq -n --arg user "$username" --argjson links "$LINKS_JSON" \
    '{username: $user, links: $links}')

  echo "$RESULT"
done
