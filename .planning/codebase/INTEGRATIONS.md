# External Integrations

**Analysis Date:** 2026-01-08

## APIs & External Services

**Payment Processing:**
- Not applicable

**Email/SMS:**
- Not applicable

**External APIs:**
- None currently integrated
- System is self-contained (no cloud API dependencies)

## Data Storage

**Databases:**
- None currently (data transmitted, not stored)
- Future: SQLite for persistent message queue (per `docs/PROPOSAL.md`)

**File Storage:**
- None currently
- Future: Local filesystem for survey file chunks

**Caching:**
- None (no caching layer implemented)

## Authentication & Identity

**Auth Provider:**
- None currently
- Future: HMAC-based device authentication (per `docs/PROPOSAL.md`)

**OAuth Integrations:**
- Not applicable

## Monitoring & Observability

**Error Tracking:**
- None (uses `println!` for basic output)
- Future: Structured logging with tracing crate

**Analytics:**
- Not applicable

**Logs:**
- stdout only via `println!` macros
- Files: `src/main.rs`, `relay-node/src/main.rs`, `server/src/main.rs`

## CI/CD & Deployment

**Hosting:**
- Self-hosted on Raspberry Pi 5 devices
- Manual deployment (no automated pipeline)

**CI Pipeline:**
- Not configured
- No `.github/workflows/` directory

## Environment Configuration

**Development:**
- No environment variables required
- All configuration hardcoded in source files
- Run with `cargo run --bin edge-device`, `cargo run --bin relay-node`, `cargo run --bin server`

**Staging:**
- Not applicable (embedded deployment)

**Production:**
- Raspberry Pi 5 with Raspberry Pi OS
- Binary deployment via cargo build --release

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- None

## Hardware Integrations

**Network Interfaces (Future):**
- 5G USB Modem - Primary transport (QMI/MBIM via kernel drivers)
  - Currently simulated: TCP to `127.0.0.1:8080` (`src/transport/five_g.rs`)
  - Future: ModemManager D-Bus integration with `zbus` crate

- Bluetooth (BlueZ) - Fallback mesh relay
  - Currently simulated: TCP to `127.0.0.1:9000` (`src/transport/bluetooth.rs`)
  - Future: Real Bluetooth via `bluer` crate

**Sensor Data (Future):**
- GPS module - Telemetry data (50 bytes, 1-2 Hz)
- GPR (Ground Penetrating Radar) - Survey files (50MB-2GB)
- LiDAR - Point cloud scans (100MB-10GB)

## Protocol & Serialization

**Protocol Buffers (Protobuf):**
- Used for message encoding across all components
- Library: `prost = "0.14.1"`
- Message definition: `SensorPacket` in `src/protocol.rs`
- Fields: device_id (string), timestamp (u64), payload (string)

## Communication Pattern

**TCP Sockets:**
- Edge → 5G gateway: `127.0.0.1:8080` (primary)
- Edge → Bluetooth relay: `127.0.0.1:9000` (fallback)
- Relay → Server: `127.0.0.1:8080` (forwarding)
- Server listens: `0.0.0.0:8080`
- Relay listens: `0.0.0.0:9000`

---

*Integration audit: 2026-01-08*
*Update when adding/removing external services*
