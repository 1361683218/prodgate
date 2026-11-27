//! prodgate: a minimal production-readiness gate.
//!
//! The gate checks a project for a small set of "invisible controls" that
//! senior engineers add by reflex and models almost never add on their own:
//! error tracking, rate limiting, webhook idempotency, RLS, wildcard CORS,
//! secret handling, backup declarations.
//!
//! Design constraint: the gate must pass on its own repository. Every rule
//! carries a (good, bad) fixture pair so the rule library is replayable.

pub mod adapter;
pub mod artifacts;
pub mod cases;
pub mod check;
pub mod efficacy;
pub mod fixture;
pub mod gate;
pub mod history;
pub mod html_report;
pub mod json;
pub mod mcp;
pub mod mutation;
pub mod prompt;
pub mod regression;
pub mod report;
pub mod rules_security;
