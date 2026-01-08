# Technology Stack

**Analysis Date:** 2026-01-08

## Languages

**Primary:**
- Rust (Edition 2021) - All application code (`src/**/*.rs`, `relay-node/src/main.rs`, `server/src/main.rs`)

**Secondary:**
- None

## Runtime

**Environment:**
- Linux (aarch64 target) - Raspberry Pi 5 primary deployment
- Kernel: 6.1+ (Raspberry Pi OS 64-bit Bookworm)
- ARM Cortex-A76 (4-core, 2.4GHz on Pi 5)

**Package Manager:**
- Cargo (Rust package manager)
- Lockfile: `Cargo.lock` gitignored (standard for binary applications)

## Frameworks

**Core:**
- None (vanilla async Rust)

**Async Runtime:**
- tokio 1.x with "full" features - Async runtime for networking (`Cargo.toml`)

**Testing:**
- None configured (early prototype stage)

**Build/Dev:**
- Cargo (Rust default build system)
- No additional build tooling

## Key Dependencies

**Critical:**
- `tokio = { version = "1", features = ["full"] }` - Async runtime, TCP networking, timers (`Cargo.toml`, `relay-node/Cargo.toml`, `server/Cargo.toml`)
- `prost = "0.14.1"` - Protocol Buffers serialization (`Cargo.toml`, `server/Cargo.toml`)
- `anyhow = "1"` - Ergonomic error handling (`Cargo.toml`, `relay-node/Cargo.toml`, `server/Cargo.toml`)

**Infrastructure:**
- Tokio TcpListener/TcpStream - TCP socket abstraction
- No database drivers (data is transmitted, not stored)

## Configuration

**Environment:**
- No environment variables required (hardcoded configuration)
- No `.env` files (`.gitignore` excludes them for future use)
- All addresses hardcoded in source:
  - Server: `0.0.0.0:8080` (`server/src/main.rs`)
  - Relay: `0.0.0.0:9000` (`relay-node/src/main.rs`)
  - Edge to 5G: `127.0.0.1:8080` (`src/transport/five_g.rs`)
  - Edge to BT: `127.0.0.1:9000` (`src/transport/bluetooth.rs`)

**Build:**
- `Cargo.toml` - Workspace root manifest
- `relay-node/Cargo.toml` - Relay node package
- `server/Cargo.toml` - Server package

## Platform Requirements

**Development:**
- Any platform with Rust toolchain
- `cargo build` to compile
- `cargo run` for each component

**Production:**
- Raspberry Pi 5 (aarch64-unknown-linux-gnu target)
- Raspberry Pi OS 64-bit Bookworm (Kernel 6.1+)
- Future: 5G USB modem (QMI/MBIM drivers)
- Future: Bluetooth 5.0 (BlueZ stack)

---

*Stack analysis: 2026-01-08*
*Update after major dependency changes*
