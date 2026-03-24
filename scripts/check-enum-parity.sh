#!/bin/bash
# BND-004: Enforce canonical enum parity between Rust definitions and SQL CHECK constraints.
#
# Extracts:
#   1. Enum names + variants from packages/state-model/src/lib.rs
#   2. CHECK constraints from db/migrations/*.sql
# Compares them and reports mismatches.
#
# Usage: bash scripts/check-enum-parity.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STATE_MODEL="$PROJECT_ROOT/packages/state-model/src/lib.rs"
MIGRATION_DIR="$PROJECT_ROOT/db/migrations"

if [[ ! -f "$STATE_MODEL" ]]; then
    echo "ERROR: $STATE_MODEL not found"
    exit 1
fi

if [[ ! -d "$MIGRATION_DIR" ]]; then
    echo "ERROR: $MIGRATION_DIR not found"
    exit 1
fi

echo "=== BND-004: Enum Parity Check ==="
echo ""

# ── Step 1: Extract Rust enums from state-model ──
# Captures blocks like:
#   pub enum Foo {
#       VariantA,
#       VariantB,
#   }
# Converts PascalCase variants to snake_case via sed.
declare -A RUST_ENUMS

current_enum=""
while IFS= read -r line; do
    # Match "pub enum EnumName {"
    if echo "$line" | grep -qE '^\s*pub enum [A-Z]'; then
        current_enum=$(echo "$line" | sed -n 's/.*pub enum \([A-Za-z_]*\).*/\1/p')
        RUST_ENUMS[$current_enum]=""
        continue
    fi

    # Inside an enum block, capture variants
    if [[ -n "$current_enum" ]]; then
        # End of enum
        if echo "$line" | grep -qE '^\s*\}'; then
            current_enum=""
            continue
        fi

        # Capture variant name (before comma, paren, or brace)
        variant=$(echo "$line" | sed -n 's/^\s*\([A-Za-z][A-Za-z0-9]*\)\s*[,({].*/\1/p')
        if [[ -z "$variant" ]]; then
            variant=$(echo "$line" | sed -n 's/^\s*\([A-Za-z][A-Za-z0-9]*\)\s*$/\1/p')
        fi

        if [[ -n "$variant" ]]; then
            # Convert PascalCase to snake_case
            snake=$(echo "$variant" | sed -e 's/\([A-Z]\)/_\L\1/g' -e 's/^_//')
            if [[ -n "${RUST_ENUMS[$current_enum]}" ]]; then
                RUST_ENUMS[$current_enum]="${RUST_ENUMS[$current_enum]} $snake"
            else
                RUST_ENUMS[$current_enum]="$snake"
            fi
        fi
    fi
done < "$STATE_MODEL"

# ── Step 2: Extract CHECK constraints from migrations ──
# Matches patterns like:
#   CHECK (column_name IN ('val1','val2','val3'))
# Captures the constraint name (from ALTER TABLE ... ADD CONSTRAINT) and values.
declare -A SQL_CHECKS

for sql_file in "$MIGRATION_DIR"/*.sql; do
    [[ -f "$sql_file" ]] || continue

    # Extract CHECK constraint values.
    # Look for lines with CHECK (... IN (...))
    while IFS= read -r line; do
        # Match constraint name from ADD CONSTRAINT lines
        if echo "$line" | grep -qi 'ADD CONSTRAINT'; then
            constraint_name=$(echo "$line" | sed -n "s/.*ADD CONSTRAINT \([a-z_]*\).*/\1/p")
        fi

        # Match CHECK (...IN (...)) values
        if echo "$line" | grep -qi 'CHECK.*IN'; then
            # Extract the column name
            col_name=$(echo "$line" | sed -n "s/.*CHECK\s*(\s*\([a-z_]*\)\s*IN.*/\1/p")
            # Extract the values inside IN(...)
            values_raw=$(echo "$line" | sed -n "s/.*IN\s*(\(.*\))/\1/p" | tr -d "'" | tr -d " ")
            if [[ -n "$col_name" && -n "$values_raw" ]]; then
                key="${constraint_name:-${col_name}}"
                SQL_CHECKS[$key]=$(echo "$values_raw" | tr ',' ' ')
            fi
        fi
    done < "$sql_file"
done

# ── Step 3: Map Rust enums to SQL CHECK constraints ──
# Known mappings from the canonical registry
declare -A ENUM_TO_CHECK
ENUM_TO_CHECK[EventKind]="chk_event_journal_event_kind"
ENUM_TO_CHECK[PlanGate]="chk_objectives_plan_gate"
ENUM_TO_CHECK[NodeLane]="chk_nodes_lane"
ENUM_TO_CHECK[NodeLifecycle]="chk_nodes_lifecycle"
ENUM_TO_CHECK[TaskStatus]="chk_tasks_status"
ENUM_TO_CHECK[CyclePhase]="chk_cycles_phase"

MISMATCH_COUNT=0
CHECKED_COUNT=0
MISSING_SQL_COUNT=0

echo "Enum parity results:"
echo "-----------------------------"

for enum_name in "${!ENUM_TO_CHECK[@]}"; do
    check_name="${ENUM_TO_CHECK[$enum_name]}"
    rust_variants="${RUST_ENUMS[$enum_name]:-}"
    sql_variants="${SQL_CHECKS[$check_name]:-}"

    if [[ -z "$rust_variants" ]]; then
        echo "  [WARN]    $enum_name: no Rust variants found in state-model"
        continue
    fi

    if [[ -z "$sql_variants" ]]; then
        echo "  [MISSING] $enum_name: no SQL CHECK constraint found ($check_name)"
        MISSING_SQL_COUNT=$((MISSING_SQL_COUNT + 1))
        continue
    fi

    CHECKED_COUNT=$((CHECKED_COUNT + 1))

    # Sort both for comparison
    rust_sorted=$(echo "$rust_variants" | tr ' ' '\n' | sort)
    sql_sorted=$(echo "$sql_variants" | tr ' ' '\n' | sort)

    if [[ "$rust_sorted" == "$sql_sorted" ]]; then
        echo "  [OK]      $enum_name  ($check_name)"
    else
        echo "  [MISMATCH] $enum_name"
        MISMATCH_COUNT=$((MISMATCH_COUNT + 1))

        # Find differences
        rust_only=$(comm -23 <(echo "$rust_sorted") <(echo "$sql_sorted") | tr '\n' ', ' | sed 's/,$//')
        sql_only=$(comm -13 <(echo "$rust_sorted") <(echo "$sql_sorted") | tr '\n' ', ' | sed 's/,$//')

        if [[ -n "$rust_only" ]]; then
            echo "             Rust only: $rust_only"
        fi
        if [[ -n "$sql_only" ]]; then
            echo "             SQL only:  $sql_only"
        fi
    fi
done

echo ""
echo "-----------------------------"
echo "Enums checked:      $CHECKED_COUNT"
echo "Matching:           $((CHECKED_COUNT - MISMATCH_COUNT))"
echo "Mismatches:         $MISMATCH_COUNT"
echo "Missing SQL CHECK:  $MISSING_SQL_COUNT"
echo ""

if [[ $MISMATCH_COUNT -gt 0 || $MISSING_SQL_COUNT -gt 0 ]]; then
    echo "ACTION: Enum parity violations detected."
    echo "  - Mismatches: Update SQL CHECK constraints or Rust enum to match."
    echo "  - Missing CHECKs: Add CHECK constraints via a new migration."
    echo "  - See docs/contracts/CANONICAL_ENUM_REGISTRY.md for the authoritative list."
    exit 1
else
    echo "All mapped enums are in parity between Rust and SQL."
    exit 0
fi
