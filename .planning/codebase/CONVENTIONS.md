# Coding Conventions

**Analysis Date:** 2026-01-08

## Naming Patterns

**Files:**
- snake_case for all Rust files (`protocol.rs`, `five_g.rs`, `bluetooth.rs`)
- `main.rs` for binary entry points
- `mod.rs` for module manifests
- UPPERCASE.md for important documentation (`README.md`, `PROPOSAL.md`)

**Functions:**
- snake_case for all functions (`encode`, `decode`, `send`)
- Async functions use same naming (no special prefix)
- Example: `pub async fn send(data: &[u8]) -> anyhow::Result<()>` in `src/transport/five_g.rs`

**Variables:**
- snake_case for variables (`device_id`, `timestamp`, `payload`)
- No underscore prefix for private members
- Example: `src/protocol.rs` struct fields

**Types:**
- PascalCase for structs (`SensorPacket` in `src/protocol.rs`)
- PascalCase for enums (none currently defined)
- No I prefix for interfaces/traits

## Code Style

**Formatting:**
- 4-space indentation (Rust default)
- No tabs observed
- Uses rustfmt default conventions
- No explicit `.rustfmt.toml` configuration

**Line Length:**
- No explicit limit configured
- Follows rustfmt defaults (~100 characters)

**Quotes:**
- Double quotes for string literals
- Example: `device_id: "edge-001".into()` in `src/main.rs`

**Semicolons:**
- Required at end of statements (Rust language requirement)
- Expression returns without semicolons where appropriate

**Linting:**
- No explicit clippy configuration
- Uses Rust default warnings
- No `.clippy.toml` found

## Import Organization

**Order:**
1. Local module declarations (`mod protocol;`, `mod transport;`)
2. Standard library imports (`use std::time::{...}`)
3. Third-party imports (`use tokio::...`, `use prost::...`)
4. Local module imports (`use protocol::*;`)

**Grouping:**
- Single blank line between module declarations and use statements
- Related imports grouped together

**Path Aliases:**
- None configured (no path aliases in Cargo.toml)
- Uses relative imports within crate

**Example from `src/main.rs`:**
```rust
mod protocol;
mod transport;

use protocol::*;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
```

## Error Handling

**Patterns:**
- `anyhow::Result<()>` for async function returns
- `.await?` propagation for async errors
- `.unwrap()` used in prototype code (tech debt)

**Error Types:**
- Uses `anyhow` crate for ergonomic error handling
- No custom error types defined
- Panics on `.unwrap()` failures (needs production hardening)

**Async:**
- try/catch pattern for transport fallback
- Example in `src/main.rs`: `if let Err(e) = ... { ... }`

## Logging

**Framework:**
- `println!` macros only (no structured logging)
- Files: `src/main.rs`, `relay-node/src/main.rs`, `server/src/main.rs`

**Patterns:**
- Simple string interpolation
- Example: `println!("Received from {}: {:?}", packet.device_id, packet.payload);`

**Future:**
- Per `docs/PROPOSAL.md`: Should use `tracing` crate for structured logging

## Comments

**When to Comment:**
- Inline comments for implementation notes
- Example: `// simulate: try 5G first, fallback to BT` in `src/main.rs`

**JSDoc/TSDoc:**
- Not applicable (Rust project)

**Rust Doc Comments:**
- Not used currently (no `///` comments found)
- Would be `///` for public API documentation

**TODO Comments:**
- Not found in current codebase
- Would use `// TODO:` format if needed

## Function Design

**Size:**
- Functions are short (under 30 lines)
- Main loops contain inline logic

**Parameters:**
- Minimal parameters (1-2 typical)
- Example: `pub async fn send(data: &[u8])` - single slice parameter

**Return Values:**
- `anyhow::Result<()>` for fallible operations
- `Vec<u8>` for encoding functions
- Early returns not heavily used

## Module Design

**Exports:**
- `pub mod` for public submodules
- `pub fn` for public functions
- Wildcard re-exports (`pub mod bluetooth; pub mod five_g;` in `mod.rs`)

**Barrel Files:**
- `mod.rs` files re-export submodules
- Example: `src/transport/mod.rs` exports `bluetooth` and `five_g`

**Circular Dependencies:**
- None detected (clean dependency graph)

## Git Conventions

**Commit Messages:**
- Conventional commits style
- Prefix pattern: `chore:`, `init:`
- Example: `chore: initialize monorepo and add project README`

**Branch Naming:**
- `main` as default branch
- No feature branch conventions observed

---

*Convention analysis: 2026-01-08*
*Update when patterns change*
