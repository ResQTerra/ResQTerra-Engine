# Architecture

**Analysis Date:** 2026-01-08

## Pattern Overview

**Overall:** Distributed Edge Computing System (Three-Tier Microservices)

**Key Characteristics:**
- Transport-agnostic messaging with graceful degradation
- Three independent binaries: edge-device, relay-node, server
- Async-first design with Tokio runtime
- Stateless request handling (no persistent state currently)

## Layers

**Application Layer (Edge Device):**
- Purpose: Generate and transmit sensor data
- Contains: Main loop, packet creation, transport selection
- Location: `src/main.rs`
- Depends on: Protocol layer, Transport layer
- Used by: Entry point (direct execution)

**Protocol Layer:**
- Purpose: Message serialization/deserialization
- Contains: `SensorPacket` struct, encode/decode functions
- Location: `src/protocol.rs`, `server/src/main.rs` (duplicated)
- Depends on: prost crate
- Used by: Application layer, Server

**Transport Layer:**
- Purpose: Network I/O abstraction
- Contains: 5G transport, Bluetooth transport (both TCP-based currently)
- Location: `src/transport/five_g.rs`, `src/transport/bluetooth.rs`
- Depends on: tokio::net
- Used by: Application layer

**Relay Layer:**
- Purpose: Store-and-forward proxy for Bluetooth mesh
- Contains: TCP listener, forwarding logic
- Location: `relay-node/src/main.rs`
- Depends on: tokio::net, tokio::io
- Used by: Edge devices (via Bluetooth), Server

**Server Layer:**
- Purpose: Data ingestion and logging
- Contains: TCP listener, protobuf decoding
- Location: `server/src/main.rs`
- Depends on: tokio::net, prost
- Used by: Edge devices (direct), Relay nodes

## Data Flow

**Happy Path (5G Available):**

1. Edge device creates `SensorPacket` with device_id, timestamp, payload
2. Protocol layer encodes packet via `protocol::encode()`
3. Transport selector attempts 5G transport (`transport::five_g::send()`)
4. TCP connection to server:8080, data written
5. Server accepts connection, reads bytes
6. Server decodes protobuf via `prost::Message::decode()`
7. Server logs received data to stdout

**Fallback Path (5G Down, Relay Available):**

1. Edge device creates `SensorPacket`
2. Protocol layer encodes packet
3. Transport selector: 5G fails, switches to Bluetooth
4. Bluetooth transport sends to relay:9000 (`transport::bluetooth::send()`)
5. Relay accepts connection, reads bytes
6. Relay forwards to server:8080
7. Server receives and processes as normal

**State Management:**
- Stateless: No persistent state across messages
- Each message is independent
- Future: SQLite queue for message persistence

## Key Abstractions

**SensorPacket:**
- Purpose: Core data message for sensor telemetry
- Definition: `src/protocol.rs`
- Fields: device_id (string), timestamp (u64), payload (string)
- Pattern: Protobuf message with prost derive macros

**Transport (Implicit):**
- Purpose: Network send abstraction
- Implementations: `five_g::send()`, `bluetooth::send()`
- Pattern: Async functions with identical signature
- Future: Trait-based abstraction for extensibility

## Entry Points

**Edge Device:**
- Location: `src/main.rs`
- Triggers: Direct execution (`cargo run --bin edge-device`)
- Responsibilities: Generate packets every 5 seconds, implement 5G→BT fallback

**Relay Node:**
- Location: `relay-node/src/main.rs`
- Triggers: Direct execution (`cargo run --bin relay-node`)
- Responsibilities: Listen on :9000, forward received data to server:8080

**Server:**
- Location: `server/src/main.rs`
- Triggers: Direct execution (`cargo run --bin server`)
- Responsibilities: Listen on :8080, decode and log received packets

## Error Handling

**Strategy:** Fail-fast with basic fallback (prototype stage)

**Patterns:**
- `anyhow::Result<()>` return type for async functions
- `.await?` propagation in transport layer
- `.unwrap()` in prototype code (needs production hardening)
- Try/catch pattern for transport fallback in `src/main.rs`

## Cross-Cutting Concerns

**Logging:**
- Current: `println!` macros to stdout
- Files: `src/main.rs`, `relay-node/src/main.rs`, `server/src/main.rs`
- Future: Structured logging with tracing crate

**Validation:**
- Current: None (trusts all data)
- Future: Packet validation, authentication

**Authentication:**
- Current: None
- Future: HMAC-based device authentication (per `docs/PROPOSAL.md`)

---

*Architecture analysis: 2026-01-08*
*Update when major patterns change*
