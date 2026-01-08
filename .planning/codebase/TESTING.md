# Testing Patterns

**Analysis Date:** 2026-01-08

## Test Framework

**Runner:**
- None configured (early prototype stage)
- Would use: `cargo test` (Rust built-in)

**Assertion Library:**
- Not applicable (no tests)
- Would use: Rust built-in `assert!`, `assert_eq!`, `assert_ne!`

**Run Commands:**
```bash
cargo test                              # Run all tests (when implemented)
cargo test --bin edge-device            # Run edge device tests
cargo test --bin relay-node             # Run relay node tests
cargo test --bin server                 # Run server tests
cargo test -- --nocapture               # Show println! output
```

## Test File Organization

**Location:**
- Not established (no tests exist)
- Recommended: Co-located `#[cfg(test)]` modules at bottom of implementation files

**Naming:**
- Not established
- Recommended: `#[cfg(test)] mod tests { ... }` pattern

**Structure:**
```
src/
  protocol.rs           # Would contain #[cfg(test)] mod tests
  transport/
    five_g.rs           # Would contain #[cfg(test)] mod tests
    bluetooth.rs        # Would contain #[cfg(test)] mod tests
tests/                  # Integration tests (create when needed)
  integration_test.rs
```

## Test Structure

**Suite Organization (Recommended):**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        // arrange
        let packet = SensorPacket { ... };

        // act
        let encoded = encode(&packet);
        let decoded = decode(&encoded);

        // assert
        assert_eq!(packet.device_id, decoded.device_id);
    }

    #[tokio::test]
    async fn test_send_succeeds() {
        // async test with tokio runtime
    }
}
```

**Patterns:**
- Not established
- Recommended: arrange/act/assert pattern
- Recommended: `#[tokio::test]` for async tests

## Mocking

**Framework:**
- Not established
- Recommended: `mockall` crate for trait mocking

**Patterns (Recommended):**
```rust
// Define trait for transport
pub trait Transport {
    async fn send(&self, data: &[u8]) -> anyhow::Result<()>;
}

// Mock in tests
#[cfg(test)]
mod tests {
    use mockall::mock;

    mock! {
        Transport {}
        impl Transport for Transport {
            async fn send(&self, data: &[u8]) -> anyhow::Result<()>;
        }
    }
}
```

**What to Mock:**
- TCP connections (use mock transport trait)
- System time (use `std::time` abstraction)
- External network calls

**What NOT to Mock:**
- Pure functions (encode/decode)
- Simple data structures

## Fixtures and Factories

**Test Data (Recommended):**
```rust
#[cfg(test)]
mod tests {
    fn create_test_packet() -> SensorPacket {
        SensorPacket {
            device_id: "test-device".into(),
            timestamp: 1234567890,
            payload: "test payload".into(),
        }
    }
}
```

**Location:**
- Factory functions: Define in test module near usage
- Shared fixtures: Create `tests/fixtures/` if needed

## Coverage

**Requirements:**
- None established
- No coverage target configured

**Configuration:**
- Not configured
- Would use: `cargo tarpaulin` or `cargo llvm-cov`

**View Coverage (when implemented):**
```bash
cargo tarpaulin --out Html     # Generate HTML coverage report
open tarpaulin-report.html     # View report
```

## Test Types

**Unit Tests:**
- Status: NOT IMPLEMENTED
- Would test: `protocol.rs` encode/decode, transport functions
- Location: `#[cfg(test)]` modules in source files

**Integration Tests:**
- Status: NOT IMPLEMENTED
- Would test: Full pipeline (edge → relay → server)
- Location: `tests/` directory

**E2E Tests:**
- Status: NOT IMPLEMENTED
- Would test: Multi-node communication over TCP
- Framework: Would require test harness setup

## Common Patterns

**Async Testing (Recommended):**
```rust
#[tokio::test]
async fn test_async_operation() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

**Error Testing (Recommended):**
```rust
#[test]
fn test_decode_invalid_data() {
    let invalid_data = vec![0xFF, 0xFF];
    let result = std::panic::catch_unwind(|| decode(&invalid_data));
    assert!(result.is_err()); // Currently panics, should return Result
}
```

**Snapshot Testing:**
- Not used
- Not recommended for this project type

## Missing Test Infrastructure

**Critical Gaps:**
1. No `#[test]` functions in any source file
2. No `#[cfg(test)]` modules
3. No `tests/` directory for integration tests
4. No coverage tooling
5. No CI/CD test pipeline

**Priority Tests to Add:**
1. `src/protocol.rs` - Encode/decode roundtrip tests
2. `src/transport/*.rs` - Mock connection tests
3. Integration - Edge → Server data flow
4. Error paths - Network failure handling

---

*Testing analysis: 2026-01-08*
*Update when test patterns change*
