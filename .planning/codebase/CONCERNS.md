# Codebase Concerns

**Analysis Date:** 2026-01-08

## Tech Debt

**Hardcoded Configuration:**
- Issue: All network addresses hardcoded in source files
- Files: `src/transport/five_g.rs:5`, `src/transport/bluetooth.rs:5`, `relay-node/src/main.rs:6,15`, `server/src/main.rs:19`
- Why: Rapid prototyping without config infrastructure
- Impact: Cannot reconfigure without recompilation
- Fix approach: Add environment variable support or config file (e.g., `config` crate)

**Protocol Duplication:**
- Issue: `SensorPacket` struct defined in both edge device and server
- Files: `src/protocol.rs`, `server/src/main.rs`
- Why: No shared library crate established
- Impact: Changes require updates in multiple places, risk of drift
- Fix approach: Create shared `common` or `protocol` crate in workspace

**Unstructured Logging:**
- Issue: Uses `println!` instead of structured logging
- Files: `src/main.rs:24`, `relay-node/src/main.rs:7`, `server/src/main.rs:20,29-32`
- Why: Prototype simplicity
- Impact: Difficult to filter, parse, or monitor logs in production
- Fix approach: Integrate `tracing` crate with JSON output

## Known Bugs

**No Known Bugs Currently:**
- Codebase is early prototype
- Bugs would manifest in production deployment

## Security Considerations

**No Authentication:**
- Risk: Any device can send data to server, no identity verification
- Files: All network endpoints accept unauthenticated connections
- Current mitigation: None
- Recommendations: Implement HMAC device authentication per `docs/PROPOSAL.md`

**No TLS/Encryption:**
- Risk: Data transmitted in plaintext over network
- Files: `src/transport/five_g.rs`, `src/transport/bluetooth.rs`, `relay-node/src/main.rs`, `server/src/main.rs`
- Current mitigation: None (localhost-only in prototype)
- Recommendations: Add `rustls` for TLS transport

**No Input Validation:**
- Risk: Malformed protobuf data could cause undefined behavior
- Files: `server/src/main.rs:28` - decodes without validation
- Current mitigation: Trusts all input
- Recommendations: Add packet validation before processing

## Performance Bottlenecks

**Fixed Buffer Size:**
- Problem: 1KB fixed buffer may truncate large messages
- Files: `relay-node/src/main.rs:12`, `server/src/main.rs:25`
- Measurement: Any message >1024 bytes silently truncated
- Cause: Hardcoded buffer `let mut buf = [0u8; 1024];`
- Improvement path: Use length-prefix framing or growable buffer

**Unbounded Task Spawning:**
- Problem: No limit on concurrent connections
- Files: `relay-node/src/main.rs:11`, `server/src/main.rs:24`
- Measurement: High connection rate could exhaust memory
- Cause: `tokio::spawn` without backpressure
- Improvement path: Add connection semaphore or rate limiting

## Fragile Areas

**Error Handling with `.unwrap()`:**
- Why fragile: 8 `.unwrap()` calls that panic on failure
- Files:
  - `src/main.rs:15` - SystemTime unwrap
  - `src/protocol.rs:17,22` - encode/decode unwraps
  - `relay-node/src/main.rs:13,15,16` - socket operation unwraps
  - `server/src/main.rs:26,28` - socket and decode unwraps
- Common failures: Network errors, malformed data, clock issues
- Safe modification: Replace `.unwrap()` with `?` or proper error handling
- Test coverage: No tests exist

**No Graceful Shutdown:**
- Why fragile: Infinite loops with no shutdown mechanism
- Files: `src/main.rs:10-29`, `relay-node/src/main.rs:9-18`, `server/src/main.rs:22-34`
- Common failures: SIGTERM kills process mid-operation
- Safe modification: Add signal handler with tokio::signal
- Test coverage: No tests

## Scaling Limits

**Single-Threaded Accept Loop:**
- Current capacity: Unknown (not benchmarked)
- Limit: Single-threaded accept may bottleneck under load
- Symptoms at limit: Connection queuing, timeouts
- Scaling path: Consider multi-threaded accept or connection pool

**No Message Queue:**
- Current capacity: 0 messages (no persistence)
- Limit: Network failure = data loss
- Symptoms at limit: Lost sensor data during outages
- Scaling path: Add SQLite queue per `docs/PROPOSAL.md`

## Dependencies at Risk

**prost 0.14.1:**
- Risk: Relatively new version, API may change
- Impact: Protobuf serialization
- Migration plan: Pin version, test before upgrading

## Missing Critical Features

**Persistent Queue:**
- Problem: No message persistence, data lost on crash
- Current workaround: None (messages lost)
- Blocks: Reliable delivery guarantee
- Implementation complexity: Medium (SQLite integration)

**Length-Prefix Framing:**
- Problem: TCP stream has no message boundaries
- Current workaround: One message per connection
- Blocks: Multiple messages on same connection
- Implementation complexity: Low (add 4-byte length prefix)

**Transport Abstraction Trait:**
- Problem: Transports are separate functions, not polymorphic
- Current workaround: Manual selection in main.rs
- Blocks: Easy addition of new transport types
- Implementation complexity: Low (define trait, implement)

## Test Coverage Gaps

**Protocol Layer:**
- What's not tested: encode/decode functions in `src/protocol.rs`
- Risk: Serialization bugs could corrupt data
- Priority: High
- Difficulty to test: Low (pure functions, easy to test)

**Transport Layer:**
- What's not tested: Network send functions in `src/transport/*.rs`
- Risk: Network handling bugs undetected
- Priority: High
- Difficulty to test: Medium (requires mock connections)

**Full Pipeline:**
- What's not tested: Edge → Relay → Server data flow
- Risk: Integration issues between components
- Priority: High
- Difficulty to test: Medium (requires test harness)

**Error Paths:**
- What's not tested: Behavior when network fails, data malformed
- Risk: Panics or undefined behavior in production
- Priority: Critical
- Difficulty to test: Medium (mock failure conditions)

---

*Concerns audit: 2026-01-08*
*Update as issues are fixed or new ones discovered*
