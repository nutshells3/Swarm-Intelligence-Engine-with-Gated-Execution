#!/bin/bash
# Check OpenAPI coverage: compare registered routes in the axum Router
# against #[utoipa::path] annotations in route handler files.
#
# Reports routes that exist in the Router but have no OpenAPI documentation.
#
# Usage: bash scripts/check-api-coverage.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LIB_RS="$PROJECT_ROOT/services/orchestration-api/src/lib.rs"
ROUTE_DIR="$PROJECT_ROOT/services/orchestration-api/src/routes"

if [[ ! -f "$LIB_RS" ]]; then
    echo "ERROR: $LIB_RS not found"
    exit 1
fi

echo "=== API Coverage Check ==="
echo ""

# ── Step 1: Extract all route paths from lib.rs ──
# Pull every quoted string that looks like /api/... or /health from lib.rs,
# excluding internal paths like /swagger-ui and /api-docs.
ROUTE_PATHS=$(grep -o '"[^"]*"' "$LIB_RS" \
    | grep '^"/\(api\|health\)' \
    | sed 's/"//g' \
    | grep -v '/api-docs' \
    | sort -u)

# ── Step 2: Extract all documented paths from #[utoipa::path] annotations ──
OPENAPI_PATHS=""
for rs_file in "$ROUTE_DIR"/*.rs "$LIB_RS"; do
    [[ -f "$rs_file" ]] || continue
    paths=$(grep 'path = "' "$rs_file" | sed -n 's/.*path = "\([^"]*\)".*/\1/p')
    if [[ -n "$paths" ]]; then
        OPENAPI_PATHS="$OPENAPI_PATHS
$paths"
    fi
done
OPENAPI_PATHS=$(echo "$OPENAPI_PATHS" | sort -u | sed '/^$/d')

# ── Step 3: Compare ──
MISSING_COUNT=0
REGISTERED_COUNT=0
TOTAL_ROUTES=0
MISSING_LIST=""

echo "Route coverage:"
echo "-----------------------------"
while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    TOTAL_ROUTES=$((TOTAL_ROUTES + 1))

    # Normalize path params for matching
    normalized=$(echo "$path" | sed 's/{[^}]*}/{_}/g')
    found=false

    while IFS= read -r openapi_path; do
        [[ -z "$openapi_path" ]] && continue
        openapi_norm=$(echo "$openapi_path" | sed 's/{[^}]*}/{_}/g')
        if [[ "$normalized" == "$openapi_norm" ]]; then
            found=true
            break
        fi
    done <<< "$OPENAPI_PATHS"

    if $found; then
        echo "  [OK]      $path"
        REGISTERED_COUNT=$((REGISTERED_COUNT + 1))
    else
        echo "  [MISSING] $path"
        MISSING_COUNT=$((MISSING_COUNT + 1))
        MISSING_LIST="$MISSING_LIST  - $path
"
    fi
done <<< "$ROUTE_PATHS"

echo ""
echo "-----------------------------"
echo "Total routes:     $TOTAL_ROUTES"
echo "With OpenAPI doc: $REGISTERED_COUNT"
echo "Missing from doc: $MISSING_COUNT"
echo ""

if [[ $MISSING_COUNT -gt 0 ]]; then
    echo "Routes missing #[utoipa::path] annotation:"
    echo "$MISSING_LIST"
    echo "ACTION: Add #[utoipa::path] annotations to these route handlers"
    echo "        and register them in the #[openapi(paths(...))] block in lib.rs."
    exit 1
else
    echo "All routes have OpenAPI documentation."
    exit 0
fi
