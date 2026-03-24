.PHONY: dev db api web loop cli check test stop sync-types check-types check-api-coverage check-enums

DB_URL := postgres://postgres:postgres@localhost/development_swarm

# ── One command to rule them all ──────────────────────────
dev: db
	@echo ""
	@echo "  Development Swarm IDE"
	@echo "  ─────────────────────"
	@echo "  API:     http://127.0.0.1:8845"
	@echo "  Swagger: http://127.0.0.1:8845/swagger-ui/"
	@echo "  Web:     http://localhost:5173"
	@echo "  Workers: worker-dispatch (auto)"
	@echo "  REPL:    (별도 터미널) make cli"
	@echo ""
	@ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/orchestration-api/Cargo.toml & \
	 ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/loop-runner/Cargo.toml & \
	 ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/worker-dispatch/Cargo.toml & \
	 cd apps/web && npm run dev & \
	 wait

# ── Individual targets ────────────────────────────────────
db:
	@docker compose up -d postgres
	@echo "Waiting for PostgreSQL..."
	@sleep 3
	@echo "PostgreSQL ready on localhost:5432"

api: db
	ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/orchestration-api/Cargo.toml

loop: db
	ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/loop-runner/Cargo.toml

web:
	cd apps/web && npm run dev

dispatch: db
	ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/worker-dispatch/Cargo.toml

cli: db
	@ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/orchestration-api/Cargo.toml &
	@ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/loop-runner/Cargo.toml &
	@ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/worker-dispatch/Cargo.toml &
	@sleep 4
	@cd apps/cli && cargo run

demo: db
	@echo ""
	@echo "  SIEGE Demo Mode"
	@echo "  ────────────────"
	@echo "  API:     http://127.0.0.1:8845"
	@echo "  Web:     http://localhost:5173"
	@echo "  Mode:    Mock adapter (no LLM calls)"
	@echo ""
	@SIEGE_DEMO_MODE=1 ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/orchestration-api/Cargo.toml & \
	 SIEGE_DEMO_MODE=1 ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/loop-runner/Cargo.toml & \
	 SIEGE_DEMO_MODE=1 ORCHESTRATION_DATABASE_URL=$(DB_URL) cargo run --manifest-path services/worker-dispatch/Cargo.toml & \
	 cd apps/web && npm run dev & \
	 wait

sync-types:
	@echo "Generating TypeScript types from OpenAPI spec..."
	@bash scripts/sync-types.sh

check-types:
	@echo "Checking API type sync..."
	@bash scripts/sync-types.sh
	@cd apps/web && npx tsc --noEmit
	@echo "Types in sync!"

check-api-coverage:
	@echo "Checking OpenAPI route coverage..."
	@bash scripts/check-api-coverage.sh

check-enums:
	@echo "Checking enum parity (Rust vs SQL)..."
	@bash scripts/check-enum-parity.sh

check:
	SQLX_OFFLINE=true cargo check

test:
	SQLX_OFFLINE=true cargo test --workspace

stop:
	@-pkill -f orchestration-api 2>/dev/null
	@-pkill -f loop-runner 2>/dev/null
	@-pkill -f worker-dispatch 2>/dev/null
	@-pkill -f "vite" 2>/dev/null
	@docker compose down
	@echo "All stopped."
