#!/bin/bash

# check-pages.sh
# Usage: ./check-pages.sh <domain>
# Example: ./check-pages.sh https://example.com

if [ $# -ne 1 ]; then
  echo "Usage: $0 <domain>"
  echo "Example: $0 https://example.com"
  exit 1
fi

# Remove trailing slash if present
DOMAIN="${1%/}"

# Wait for server to be ready
MAX_RETRIES=30
RETRY_INTERVAL=2
echo "Waiting for server at $DOMAIN to be ready..."

for i in $(seq 1 $MAX_RETRIES); do
  if curl -s -f -o /dev/null "$DOMAIN/"; then
    echo "Server is ready!"
    break
  fi

  if [ $i -eq $MAX_RETRIES ]; then
    echo "Server failed to start after $((MAX_RETRIES * RETRY_INTERVAL)) seconds"
    exit 1
  fi

  echo "Attempt $i/$MAX_RETRIES - Server not ready, waiting ${RETRY_INTERVAL}s..."
  sleep $RETRY_INTERVAL
done

# List of paths to check (without domain)
PATHS=(
  "/"
  "/apps"
  "/app/0"
  "/relays"
  # Add more paths as needed
)

failed_pages=()

for path in "${PATHS[@]}"; do
  url="${DOMAIN}${path}"
  echo "Checking $url..."

  # Fetch the page and store the response
  response=$(curl -s "$url")

  # Check HTTP status code
  if ! curl -s -f -o /dev/null "$url"; then
    failed_pages+=("$url - HTTP request failed")
    continue
  fi

  # Check for failed render message
  if echo "$response" | grep -qi "failed"; then
    failed_pages+=("$url - Contains 'failed'")
    continue
  fi

  echo "✓ Successfully checked $url"
done

# Report results
if [ ${#failed_pages[@]} -ne 0 ]; then
  echo -e "\nThe following pages failed checks:"
  for failure in "${failed_pages[@]}"; do
    echo "✗ $failure"
  done
  exit 1
else
  echo -e "\nAll pages passed rendering checks!"
fi
