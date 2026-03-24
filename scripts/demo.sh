#!/bin/bash
# SIEGE Demo Scenario
# Run this after `make demo` is running

API="http://127.0.0.1:8845"

echo "=== SIEGE Demo ==="
echo ""

# Wait for API to be ready
echo "Waiting for API..."
for i in $(seq 1 30); do
    if curl -s "$API/api/health" > /dev/null 2>&1; then
        echo "API ready!"
        break
    fi
    sleep 1
done

echo ""
echo "Step 1: Creating objective..."
RESULT=$(curl -s -X POST "$API/api/objectives" \
    -H "Content-Type: application/json" \
    -d '{
        "summary": "Build a secure REST API with user authentication, database models, CRUD endpoints, and comprehensive test coverage",
        "architecture": "Rust + Axum + SQLx + JWT auth. Three-layer architecture: routes to services to models. PostgreSQL for persistence.",
        "planning_status": "active",
        "idempotency_key": "demo-obj-001"
    }')

echo "$RESULT" | python3 -m json.tool 2>/dev/null || echo "$RESULT"
OBJ_ID=$(echo "$RESULT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('objective_id',''))" 2>/dev/null)

echo ""
echo "Objective created: $OBJ_ID"
echo ""
echo "Step 2: The loop-runner will now automatically:"
echo "  1. Create a loop for the objective"
echo "  2. Create a cycle"
echo "  3. Elaborate the plan"
echo "  4. Evaluate plan gates (9 conditions)"
echo "  5. Decompose into task graph"
echo "  6. Dispatch tasks to workers"
echo "  7. Execute tasks (mock adapter)"
echo "  8. Detect conflicts"
echo "  9. Complete cycle"
echo ""
echo "Watch the web UI at http://localhost:5173"
echo "Watch the terminal logs for real-time progress"
echo ""
echo "Press Ctrl+C to stop"

# Follow the logs
while true; do
    sleep 5
    echo ""
    echo "--- Status check ---"
    curl -s "$API/api/objectives/$OBJ_ID" 2>/dev/null | python3 -m json.tool 2>/dev/null
done
