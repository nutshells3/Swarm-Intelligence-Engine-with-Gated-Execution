# Development Swarm IDE - Quick Start

## Prerequisites
- Rust 1.86+ (install via rustup)
- Node.js 20+ and pnpm 10+
- Docker (for PostgreSQL)

## Setup

1. Start PostgreSQL:

       docker compose up -d

2. Run the API server:

       cd services/orchestration-api
       ORCHESTRATION_DATABASE_URL=postgres://postgres:postgres@localhost/development_swarm cargo run

3. In another terminal, run the web frontend:

       cd apps/desktop
       pnpm install
       pnpm dev

4. Open http://localhost:5173

## CLI

Build and use the standalone CLI tool:

    cd apps/cli
    cargo run -- health
    cargo run -- status
    cargo run -- objective create --summary "Build the first feature"
    cargo run -- objective list
    cargo run -- events

## Makefile targets

    make db      # Start PostgreSQL in Docker
    make api     # Run the API server (starts db first)
    make web     # Run the web dev server
    make cli     # Build the CLI tool
    make check   # Typecheck all Rust code
    make test    # Run all workspace tests
    make dev     # Full dev setup
