//! Planning engine -- schema definitions, pipeline traits, and
//! validation rules for the M2 planning layer.
//!
//! This crate provides:
//!
//! - **Schemas** (PLAN-001 -- PLAN-009): Typed, serialisable Rust
//!   structs/enums covering every planning artefact (objective intake,
//!   architecture draft, milestone tree, dependency graph, acceptance
//!   criteria, unresolved questions, risk register, invariants, and the
//!   plan gate).
//!
//! - **Pipeline traits** (PLAN-010 -- PLAN-017): Async-ready trait
//!   definitions for each planning pipeline step (objective expansion,
//!   architecture drafting, milestone explosion, dependency extraction,
//!   acceptance criteria generation, question extraction, risk
//!   generation, and invariant extraction).  Concrete implementations
//!   will be provided by AI workers.
//!
//! - **Validation** (PLAN-018 -- PLAN-020): Deterministic functions for
//!   plan completeness scoring, structured validation failure reporting,
//!   and implementation dispatch gating.
//!
//! All types derive `Serialize` / `Deserialize` so they can be persisted
//! as JSON in the event journal or stored in Postgres via the companion
//! SQL migration (`db/migrations/0003_m2_planning_schemas.sql`).

pub mod pipeline;
pub mod schemas;
pub mod validation;

// Re-export every public type at crate root for ergonomic imports.
pub use pipeline::*;
pub use schemas::*;
pub use validation::*;
