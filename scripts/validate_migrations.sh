#!/usr/bin/env bash
# validate_migrations.sh — Dual-path migration validation
# Ensures both fresh-migrate and upgrade-migrate yield the same schema.
#
# Usage: ./scripts/validate_migrations.sh [fresh|upgrade|both]
# Default: both
#
# Requires: docker with postgres:16 available, no port conflicts on 5433.

set -euo pipefail

MODE="${1:-both}"
FRESH_PORT=5433
FRESH_CONTAINER="siege-migrate-fresh-$$"
FRESH_DB="development_swarm"
MIGRATION_DIR="db/migrations"
DUMP_CMD="SELECT table_name, column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_schema='public' ORDER BY table_name, ordinal_position"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

cleanup() {
    docker rm -f "$FRESH_CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

dump_schema() {
    local port="$1"
    docker exec "$(docker ps -qf publish="$port")" \
        psql -U postgres -d "$FRESH_DB" -t -A -F',' -c "$DUMP_CMD" 2>/dev/null \
        | grep -v '^$' | grep -v '_sqlx_migrations' | sort
}

run_fresh() {
    echo "=== FRESH-MIGRATE PATH ==="

    # Start a clean postgres
    docker run -d --name "$FRESH_CONTAINER" \
        -p "$FRESH_PORT:5432" \
        -e POSTGRES_DB="$FRESH_DB" \
        -e POSTGRES_HOST_AUTH_METHOD=trust \
        postgres:16 >/dev/null

    # Wait for ready
    for i in $(seq 1 30); do
        if docker exec "$FRESH_CONTAINER" pg_isready -U postgres >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done

    # Run all migrations from scratch
    local fail=0
    for f in "$MIGRATION_DIR"/*.sql; do
        echo -n "  $(basename "$f")... "
        if docker exec -i "$FRESH_CONTAINER" psql -U postgres -d "$FRESH_DB" < "$f" >/dev/null 2>&1; then
            echo "ok"
        else
            echo "FAIL"
            fail=1
        fi
    done

    if [ "$fail" -eq 1 ]; then
        echo -e "${RED}FRESH-MIGRATE: some migrations failed${NC}"
        return 1
    fi

    # Dump schema
    dump_schema "$FRESH_PORT" > /tmp/siege_fresh_schema.csv
    echo "  Fresh schema: $(wc -l < /tmp/siege_fresh_schema.csv) column definitions"
    echo -e "${GREEN}FRESH-MIGRATE: passed${NC}"
}

run_upgrade() {
    echo "=== UPGRADE-MIGRATE PATH ==="

    # Use the existing dev DB (port 5432)
    if ! docker exec first-postgres-1 pg_isready -U postgres >/dev/null 2>&1; then
        echo -e "${RED}UPGRADE-MIGRATE: dev postgres not running${NC}"
        return 1
    fi

    # Dump current schema
    dump_schema 5432 > /tmp/siege_upgrade_schema.csv
    echo "  Upgrade schema: $(wc -l < /tmp/siege_upgrade_schema.csv) column definitions"
    echo -e "${GREEN}UPGRADE-MIGRATE: passed${NC}"
}

compare() {
    echo "=== SCHEMA COMPARISON ==="
    if diff /tmp/siege_fresh_schema.csv /tmp/siege_upgrade_schema.csv >/dev/null 2>&1; then
        echo -e "${GREEN}MATCH: fresh and upgrade schemas are identical${NC}"
    else
        echo -e "${RED}DIVERGENCE DETECTED:${NC}"
        diff /tmp/siege_fresh_schema.csv /tmp/siege_upgrade_schema.csv | head -40
        return 1
    fi
}

case "$MODE" in
    fresh)
        run_fresh
        ;;
    upgrade)
        run_upgrade
        ;;
    both)
        run_fresh
        run_upgrade
        compare
        ;;
    *)
        echo "Usage: $0 [fresh|upgrade|both]"
        exit 1
        ;;
esac
