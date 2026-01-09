# **Ground Survey Data Collection System**
## Complete Technical Proposal & Architecture

**Project Type:** Embedded IoT / Edge Computing / Geospatial Data Pipeline  
**Target Environment:** Field deployment, intermittent connectivity  
**Primary Constraint:** Network reliability (5G → Bluetooth fallback)

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [System Overview](#system-overview)
3. [Hardware Architecture](#hardware-architecture)
4. [Software Architecture](#software-architecture)
5. [Network Architecture](#network-architecture)
6. [Data Flow & Protocol Design](#data-flow--protocol-design)
7. [Prototype Scope (Current)](#prototype-scope-current)
8. [Production Requirements (TODO)](#production-requirements-todo)
9. [Risk Assessment](#risk-assessment)
10. [Implementation Roadmap](#implementation-roadmap)

---

## Executive Summary

### The Problem

Field survey equipment (GPR, LiDAR) generates large datasets (100MB - 10GB) that must be transmitted from remote locations with unreliable connectivity. Traditional approaches either:
- Require manual data retrieval (slow, labor-intensive)
- Use satellite uplinks (expensive, low bandwidth)
- Fail silently when connectivity drops (data loss)

### The Solution

A **three-tier edge computing architecture** with intelligent transport fallback:

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│ Edge Device │ ──5G──→ │   Server    │         │   Server    │
│  (Field)    │         │  (Cloud)    │         │  (Cloud)    │
│             │         └─────────────┘         └─────────────┘
│             │                ▲                        ▲
│             │                │                        │
│             │         (when 5G fails)          (relay forwards)
│             │                │                        │
│             │ ──BT──→ ┌─────────────┐                │
│             │         │ Relay Node  │ ───────────────┘
└─────────────┘         │ (Vehicle)   │          5G
                        └─────────────┘
```

**Key Innovation:** Graceful degradation from high-bandwidth (5G) to mesh relay (Bluetooth) without data loss.

---

## System Overview

### Components

| Component | Hardware | Role | Connectivity |
|-----------|----------|------|--------------|
| **Edge Device** | Raspberry Pi 5 + 5G dongle | Data capture, local buffering, transmission | 5G (primary), Bluetooth (fallback) |
| **Relay Node** | Raspberry Pi 5 / Laptop | Store-and-forward proxy | Bluetooth (inbound), 5G/WiFi (outbound) |
| **Server** | Cloud VM (AWS/GCP/Azure) | Data ingestion, storage, processing | Internet-accessible |

### Data Classes

| Type | Size | Frequency | Latency Requirement | Loss Tolerance |
|------|------|-----------|---------------------|----------------|
| **GPS Telemetry** | 50 bytes | 1-2 Hz | < 1 second | Lossy OK (recent data preferred) |
| **Status/Heartbeat** | 200 bytes | 0.1 Hz | < 5 seconds | Lossless (must arrive eventually) |
| **GPR Data** | 50MB - 2GB | Per survey | Minutes-Hours | Lossless (must arrive complete) |
| **LiDAR Data** | 100MB - 10GB | Per scan | Minutes-Hours | Lossless (must arrive complete) |

### Design Principles

1. **Offline-First:** System operates fully when disconnected, queues data locally
2. **Fail-Fast:** Transport failures detected quickly (< 10s), fallback immediate
3. **Idempotent:** Duplicate sends are safe (server deduplicates)
4. **Observable:** Every state transition logged, metrics exported
5. **Recoverable:** Power loss or crash → resumes from checkpoint

---

## Hardware Architecture

### Edge Device (Field Unit)

```
┌───────────────────────────────────────────────────────────┐
│  Raspberry Pi 5 (8GB RAM)                                 │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  ARM Cortex-A76 (4-core, 2.4GHz)                    │  │
│  │  ─────────────────────────────────────────────────  │  │
│  │  OS: Raspberry Pi OS 64-bit (Bookworm)              │  │
│  │  Kernel: 6.1+                                       │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                           │
│  Peripherals:                                             │
│  ├─ USB 3.0: 5G Modem (Quectel RM500Q or similar)         │
│  ├─ USB 3.0: External SSD (1TB, local buffer)             │
│  ├─ GPIO: GPS module (u-blox NEO-M9N)                     │
│  ├─ Built-in: Bluetooth 5.0 (BCM43455)                    │
│  └─ MicroSD: 64GB (OS + logs)                             │
│                                                           │
│  Power:                                                   │
│  └─ 27W USB-C PD (5G modem can draw 15W peak)             │
└───────────────────────────────────────────────────────────┘
```

**Critical Hardware Decisions:**

- **8GB RAM model:** Required for buffering large files during upload
- **Active cooling:** 5G modem generates significant heat (thermal throttling risk)
- **Powered USB hub:** Direct Pi USB ports cannot reliably power high-draw 5G modems
- **External SSD:** SD cards fail under heavy write loads (survey data buffering)

### Relay Node (Optional Mobile Relay)

```
┌───────────────────────────────────────────────────────────┐
│  Raspberry Pi 5 (4GB RAM) OR Laptop                       │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  Same specs as Edge Device                          │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                           │
│  Peripherals:                                             │
│  ├─ USB 3.0: 5G Modem (for forwarding)                    │
│  ├─ Built-in: Bluetooth 5.0 (receives from edge)          │
│  └─ WiFi: Fallback connectivity                           │
│                                                           │
│  Power:                                                   │
│  └─ Vehicle 12V → USB-C converter (for mobile use)        │
└───────────────────────────────────────────────────────────┘
```

### Server (Cloud Backend)

```
┌───────────────────────────────────────────────────────────┐
│  Cloud VM (e.g., AWS EC2 t3.medium or equivalent)         │
│  ┌─────────────────────────────────────────────────────┐  │
│  │  2 vCPU, 4GB RAM                                    │  │
│  │  OS: Ubuntu Server 24.04 LTS                        │  │
│  └─────────────────────────────────────────────────────┘  │
│                                                           │
│  Storage:                                                 │
│  ├─ EBS/Block Storage: 500GB (uploaded survey data)       │
│  └─ PostgreSQL: Metadata, device state                    │
│                                                           │
│  Network:                                                 │
│  └─ Static IP, TCP port 8080 (data ingestion)             │
└───────────────────────────────────────────────────────────┘
```

---

## Software Architecture

### High-Level Stack

```
┌─────────────────────────────────────────────────────────────┐
│                      Application Layer                      │
│  ┌────────────┐  ┌────────────┐  ┌────────────────────────┐ │
│  │ GPS Stream │  │ Heartbeat  │  │ Survey Upload Manager  │ │
│  └────────────┘  └────────────┘  └────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Core Business Logic                    │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Transport Selector (5G vs Bluetooth decision)         │ │
│  │  Retry Manager (exponential backoff, circuit breaker)  │ │
│  │  Queue Manager (persistent, prioritized)               │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Protocol & Encoding                     │
│  ┌────────────────────────────────────────────────────────┐ │
│  │  Protobuf Serialization                                │ │
│  │  Framing (length-prefix)                               │ │
│  │  Checksums (CRC32 per-message, SHA-256 per-file)       │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Transport Layer                        │
│  ┌──────────────────┐         ┌──────────────────────────┐  │
│  │  5G Transport    │         │  Bluetooth Transport     │  │
│  │  (TCP/IP over    │         │  (RFCOMM socket)         │  │
│  │   wwan0)         │         │                          │  │
│  └──────────────────┘         └──────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                      Device Layer (unsafe)                  │
│  ┌──────────────────┐  ┌──────────────────────────────────┐ │
│  │ ModemManager     │  │ BlueZ D-Bus                      │ │
│  │ (5G modem ctrl)  │  │ (Bluetooth stack)                │ │
│  └──────────────────┘  └──────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│                     Linux Kernel                            │
│  qmi_wwan, cdc_wdm, option (5G drivers)                     │
│  Bluetooth HCI (Bluetooth drivers)                          │
└─────────────────────────────────────────────────────────────┘
```

### Rust Workspace Structure

```
network-stack/
│
├── Cargo.toml                    # Workspace manifest
├── .cargo/
│   └── config.toml               # Cross-compilation settings
├── proto/
│   ├── message.proto             # Wire protocol definitions
│   ├── telemetry.proto
│   └── file_transfer.proto
│
├── crates/
│   ├── core/                     # Business logic (no I/O)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── state_machine.rs  # Transport switching FSM
│   │       ├── retry.rs          # Backoff logic
│   │       └── queue.rs          # In-memory queue abstractions
│   │
│   ├── device/                   # Hardware abstraction (unsafe allowed)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── modem.rs          # ModemManager D-Bus client
│   │       ├── bluetooth.rs      # BlueZ D-Bus client
│   │       └── gps.rs            # GPS serial interface
│   │
│   ├── transport/                # Network I/O
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── five_g.rs         # TCP over wwan0
│   │       ├── bluetooth.rs      # RFCOMM socket
│   │       └── traits.rs         # Transport trait definitions
│   │
│   ├── protocol/                 # Serialization
│   │   ├── Cargo.toml
│   │   ├── build.rs              # prost-build integration
│   │   └── src/
│   │       ├── lib.rs
│   │       └── framing.rs        # Length-prefix framing
│   │
│   └── storage/                  # Persistence
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── sqlite.rs         # Queue database
│           └── file_store.rs     # Survey data on disk
│
├── apps/
│   ├── edge-node/                # Runs on field device
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs         # Configuration from file
│   │       └── telemetry.rs      # GPS/heartbeat loops
│   │
│   ├── relay-node/               # Runs on mobile relay
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       └── forward.rs        # BT → 5G forwarding logic
│   │
│   └── server/                   # Runs in cloud
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── ingest.rs         # TCP server for data
│           └── storage.rs        # Write to disk/DB
│
├── tests/
│   ├── integration/              # End-to-end tests
│   └── mocks/                    # Mock transports for testing
│
└── docs/
    ├── ARCHITECTURE.md           # This document
    ├── PROTOCOL.md               # Wire protocol spec
    └── DEPLOYMENT.md             # Installation guide
```

---

## Network Architecture

### Connectivity Scenarios

#### Scenario 1: 5G Available (Happy Path)

```
Edge Device
    │
    │ (checks wwan0 interface has IP)
    │ (ModemManager reports registered)
    │
    ├─→ TCP socket to server:8080
    │   ├─ Send GPS (every 1-2s)
    │   ├─ Send heartbeat (every 30s)
    │   └─ Upload survey chunks (when queued)
    │
Server receives directly
```

**Latency:** 20-100ms  
**Throughput:** 10-50 Mbps (typical 5G)

---

#### Scenario 2: 5G Unavailable, Relay in Range

```
Edge Device
    │
    │ (wwan0 down OR no route to server)
    │ (scans for Bluetooth devices)
    │
    ├─→ RFCOMM socket to Relay Node
    │   ├─ Send GPS (forwarded)
    │   └─ Send heartbeat (forwarded)
    │   └─ Survey data queued locally (too large for BT)
    │
Relay Node
    │
    ├─→ TCP socket to server:8080
    │   └─ Forwards received messages
    │
Server receives indirectly
```

**Latency:** 100-500ms (BT adds overhead)  
**Throughput:** 50-100 KB/s (Bluetooth SPP)  
**Limitation:** Cannot forward large files (surveys buffered on edge)

---

#### Scenario 3: Fully Offline

```
Edge Device
    │
    │ (no 5G, no relay in range)
    │
    ├─→ Writes to local SQLite queue
    │   ├─ GPS waypoints (max 10,000 points)
    │   ├─ Heartbeats (max 1,000 records)
    │   └─ Survey files (on external SSD, up to disk limit)
    │
(waits for connectivity to resume)
```

**Storage Limits:**
- GPS queue: ~500 KB (10,000 × 50 bytes)
- Heartbeat queue: ~200 KB (1,000 × 200 bytes)
- Survey files: Up to available SSD space (e.g., 800 GB on 1TB drive)

When connectivity resumes, queue drains in priority order (heartbeats → GPS → surveys).

---

### Transport Selection Logic (Finite State Machine)

```
┌─────────────────────────────────────────────────────────────┐
│                     Initial State: UNKNOWN                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │ Check 5G Status │
                    └─────────────────┘
                         │        │
              ┌──────────┘        └──────────┐
              ▼                              ▼
    ┌──────────────────┐          ┌──────────────────────┐
    │ 5G_CONNECTED     │          │ 5G_UNAVAILABLE       │
    │                  │          │                      │
    │ - Send via TCP   │          │ Check BT relays      │
    │ - Monitor signal │          └──────────────────────┘
    └──────────────────┘                     │
              │                              ▼
              │                   ┌──────────────────────┐
              │                   │ BLUETOOTH_RELAY      │
              │                   │                      │
              │                   │ - Send via RFCOMM    │
              │                   │ - Queue surveys      │
              │                   └──────────────────────┘
              │                              │
              │                              │
              └──────────────┬───────────────┘
                             ▼
                  ┌──────────────────────┐
                  │ OFFLINE              │
                  │                      │
                  │ - Buffer everything  │
                  │ - Retry every 30s    │
                  └──────────────────────┘
```

**Transition Rules:**

| From State | Event | To State | Actions |
|------------|-------|----------|---------|
| UNKNOWN | Boot | 5G_CONNECTED / 5G_UNAVAILABLE | Check interfaces |
| 5G_CONNECTED | Send timeout (>10s) | 5G_UNAVAILABLE | Mark transport dead, flush queue to disk |
| 5G_CONNECTED | Signal < -100dBm (3 consecutive) | 5G_UNAVAILABLE | Proactive switch before failure |
| 5G_UNAVAILABLE | wwan0 up + IP assigned | 5G_CONNECTED | Resume queue drain |
| 5G_UNAVAILABLE | BT device discovered | BLUETOOTH_RELAY | Establish RFCOMM, send metadata |
| BLUETOOTH_RELAY | BT disconnect | OFFLINE | Close socket, retry discovery |
| OFFLINE | Check timer (every 30s) | 5G_CONNECTED / BLUETOOTH_RELAY / OFFLINE | Re-check all transports |

**Hysteresis to prevent flapping:**
- Require 3 consecutive successful sends before marking transport "stable"
- After failure, wait 30s before retrying that transport
- Prefer last-known-good transport (sticky selection for 5 minutes)

---

## Data Flow & Protocol Design

### Message Types & Priority

| Message Type | Priority | Max Queue Size | Expiry | Retry Strategy |
|--------------|----------|----------------|--------|----------------|
| **Heartbeat** | HIGH | 1,000 | 1 hour | Exponential backoff (max 5 retries) |
| **GPS Point** | MEDIUM | 10,000 | 5 minutes | Drop oldest when full |
| **Survey Metadata** | HIGH | 10,000 | Never | Persistent retry until ack |
| **Survey Chunk** | LOW | Disk-limited | Never | Resumable upload |

### Wire Protocol (Protobuf Schemas)

**proto/message.proto:**
```protobuf
syntax = "proto3";
package protocol;

// Base envelope for all messages
message Envelope {
  string device_id = 1;          // UUID of edge device
  uint64 timestamp_us = 2;       // Microseconds since UNIX epoch
  uint64 sequence = 3;           // Monotonic counter (detect loss)
  MessageType type = 4;          // Discriminator
  bytes payload = 5;             // Type-specific payload
  bytes checksum = 6;            // CRC32 of payload
}

enum MessageType {
  HEARTBEAT = 0;
  GPS_POINT = 1;
  SURVEY_METADATA = 2;
  SURVEY_CHUNK = 3;
  DEVICE_STATUS = 4;
}
```

**proto/telemetry.proto:**
```protobuf
syntax = "proto3";
package protocol;

message Heartbeat {
  string software_version = 1;
  uint32 uptime_seconds = 2;
  TransportState active_transport = 3;
  uint32 queued_messages = 4;
  uint64 queued_bytes = 5;
}

enum TransportState {
  TRANSPORT_UNKNOWN = 0;
  TRANSPORT_5G = 1;
  TRANSPORT_BLUETOOTH = 2;
  TRANSPORT_OFFLINE = 3;
}

message GpsPoint {
  double latitude = 1;           // Decimal degrees
  double longitude = 2;
  float altitude_m = 3;
  float accuracy_m = 4;          // Horizontal dilution of precision
  uint32 satellites = 5;
}

message DeviceStatus {
  float cpu_temp_c = 1;
  uint32 battery_pct = 2;        // If battery-powered
  uint64 disk_free_bytes = 3;
  int32 signal_strength_dbm = 4; // 5G signal (if available)
}
```

**proto/file_transfer.proto:**
```protobuf
syntax = "proto3";
package protocol;

message SurveyMetadata {
  string survey_id = 1;          // UUID
  SensorType sensor_type = 2;
  uint64 file_size_bytes = 3;
  bytes sha256 = 4;              // Integrity check
  uint64 timestamp_us = 5;
  GpsCoord start_location = 6;
  GpsCoord end_location = 7;
  string filename = 8;
}

enum SensorType {
  SENSOR_GPR = 0;
  SENSOR_LIDAR = 1;
}

message GpsCoord {
  double latitude = 1;
  double longitude = 2;
}

message SurveyChunk {
  string survey_id = 1;
  uint32 chunk_id = 2;           // 0-indexed
  uint32 total_chunks = 3;
  bytes data = 4;                // Raw chunk (1MB default)
  bytes chunk_checksum = 5;      // CRC32 of this chunk
}

message ChunkAck {
  string survey_id = 1;
  uint32 chunk_id = 2;
  bool success = 3;
  optional string error = 4;
}

message SurveyComplete {
  string survey_id = 1;
  bytes computed_sha256 = 2;     // Server computes from received chunks
}
```

### Framing Protocol

All messages are length-prefixed on the wire:

```
[ 4 bytes: length (u32, big-endian) ][ N bytes: protobuf Envelope ]
```

**Example:**
```
00 00 00 42  →  66 bytes follow
<protobuf data>
```

**Rust implementation:**
```rust
async fn send_message<T: prost::Message>(
    stream: &mut TcpStream,
    msg: &T
) -> Result<()> {
    let mut buf = BytesMut::new();
    msg.encode(&mut buf)?;
    
    let len = buf.len() as u32;
    stream.write_u32(len).await?;
    stream.write_all(&buf).await?;
    stream.flush().await?;
    
    Ok(())
}

async fn recv_message<T: prost::Message + Default>(
    stream: &mut TcpStream
) -> Result<T> {
    let len = stream.read_u32().await? as usize;
    
    if len > 10_000_000 {  // 10MB limit per message
        return Err(Error::MessageTooLarge);
    }
    
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    
    T::decode(&buf[..])
}
```

---

## Prototype Scope (Current)

### What We're Building Now

**Goal:** Prove transport switching works end-to-end with simple messages.

```
✓ Edge device sends "hello" messages every 5 seconds
✓ Messages contain: device_id, timestamp, sequence number
✓ Try 5G first (TCP to server)
✓ If 5G fails, fall back to Bluetooth (TCP to relay, simulated)
✓ Relay forwards to server
✓ Server logs received messages
✓ Manually simulate failures (ip link set wwan0 down)
```

### Prototype Architecture (Simplified)

```
edge-device/
├── Cargo.toml
├── src/
│   ├── main.rs              # Main loop: send message every 5s
│   ├── transport/
│   │   ├── mod.rs
│   │   ├── five_g.rs        # TCP to server:8080
│   │   └── bluetooth.rs     # TCP to relay:9000 (simulated BT)
│   └── protocol.rs          # Protobuf message encode/decode

relay-node/
├── Cargo.toml
├── src/
│   └── main.rs              # Receive on :9000, forward to server:8080

server/
├── Cargo.toml
├── src/
│   └── main.rs              # TCP listener on :8080, log messages
```

### Prototype Limitations (Acceptable for Now)

- ❌ No real Bluetooth (simulated with TCP on LAN)
- ❌ No queue persistence (in-memory only)
- ❌ No retry logic (fail-fast)
- ❌ No GPS integration
- ❌ No survey file uploads
- ❌ No ModemManager integration (manual `ip link` checks)
- ❌ No authentication/encryption
- ❌ No observability (println! only)

### Success Criteria for Prototype

1. ✅ Message successfully delivered via 5G
2. ✅ Failover to relay within 10 seconds when 5G disabled
3. ✅ Message successfully forwarded via relay
4. ✅ Failback to 5G when re-enabled
5. ✅ No message loss during single-failure scenarios
6. ✅ Code runs on actual Raspberry Pi 5

### Prototype Timeline

| Week | Milestone |
|------|-----------|
| 1 | Workspace setup, protobuf compilation, basic TCP client/server |
| 2 | Edge device → Server (5G path only) |
| 3 | Relay node forwarding logic |
| 4 | Transport switching (5G → relay fallback) |
| 5 | Deploy to real Pi 5, test with real wwan0 |

---

## Production Requirements (TODO)

These are **OUT OF SCOPE** for the prototype but **REQUIRED** for production deployment.

### Phase 1: Core Functionality (Post-Prototype)

#### 1.1 Real Bluetooth Transport
```rust
// TODO: Replace TCP simulation with RFCOMM
// Dependencies: 
//   - bluer (async Bluetooth for Rust)
//   - BlueZ 5.50+ on system
// Complexity: MEDIUM (2 weeks)
// Risk: Bluetooth pairing UX, device discovery reliability

use bluer::rfcomm::Stream;

impl Bluetooth {
    async fn connect() -> Result<Stream> {
        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;
        
        // Scan for relay nodes
        let devices = discover_relay_nodes(&adapter).await?;
        
        // Connect to best signal strength
        let device = devices.into_iter()
            .max_by_key(|d| d.rssi)
            .ok_or(Error::NoRelayFound)?;
        
        let stream = device.connect_rfcomm(RELAY_SERVICE_UUID).await?;
        Ok(stream)
    }
}
```

#### 1.2 ModemManager Integration
```rust
// TODO: D-Bus client for querying modem state
// Dependencies:
//   - zbus (async D-Bus for Rust)
//   - ModemManager 1.18+ on system
// Complexity: MEDIUM (1 week)
// Risk: Vendor-specific modem quirks

use zbus::Connection;

struct ModemMonitor {
    conn: Connection,
}

impl ModemMonitor {
    async fn is_registered(&self) -> Result<bool> {
        let proxy = ModemProxy::new(&self.conn).await?;
        let state = proxy.state().await?;
        Ok(state == ModemState::Registered)
    }
    
    async fn signal_strength(&self) -> Result<i32> {
        let proxy = ModemProxy::new(&self.conn).await?;
        proxy.signal_quality().await
    }
}
```

#### 1.3 Queue Persistence (SQLite)
```sql
-- TODO: Schema for persistent queue
-- Complexity: LOW (3 days)
-- Risk: Disk I/O performance on SD card (use external SSD)

CREATE TABLE message_queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_type TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 50,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    next_retry_at INTEGER,
    state TEXT NOT NULL DEFAULT 'pending'  -- pending, sending, sent, failed
);

CREATE INDEX idx_next_message ON message_queue(priority DESC, created_at ASC)
    WHERE state = 'pending';

CREATE TABLE survey_files (
    survey_id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    chunks_total INTEGER NOT NULL,
    chunks_sent TEXT NOT NULL,  -- JSON array of sent chunk IDs
    created_at INTEGER NOT NULL,
    completed_at INTEGER
);
```

```rust
// TODO: SQLite async wrapper
use sqlx::SqlitePool;

struct MessageQueue {
    pool: SqlitePool,
}

impl MessageQueue {
    async fn enqueue(&self, msg: Envelope) -> Result<()> {
        sqlx::query!(
            "INSERT INTO message_queue (message_type, payload, created_at)
             VALUES (?, ?, ?)",
            msg.type_,
            msg.payload,
            msg.timestamp_us
        ).execute(&self.pool
        ).execute(&self.pool).await?;
        Ok(())
    }

    async fn dequeue_next(&self) -> Result<Option<Envelope>> {
        let row = sqlx::query!(
            r#"
            SELECT id, payload
            FROM message_queue
            WHERE state = 'pending'
              AND (next_retry_at IS NULL OR next_retry_at <= ?)
            ORDER BY priority DESC, created_at ASC
            LIMIT 1
            "#,
            current_time_us()
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => {
                let env = Envelope::decode(r.payload.as_slice())?;
                Ok(Some(env))
            }
            None => Ok(None),
        }
    }

    async fn mark_sent(&self, id: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE message_queue SET state = 'sent' WHERE id = ?",
            id
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

---

### 1.4 Resumable File Uploads (Chunked)

**Why:** Survey files are too large and networks are unreliable.

**Design:**

* Fixed-size chunks (default 1 MB)
* Chunk-level ACKs
* Resume from last confirmed chunk
* Final SHA-256 verification on server

**Flow:**

```
Edge:
  ├─ Send SurveyMetadata
  ├─ Wait for server ACK
  ├─ For chunk_id in [0..N):
  │     ├─ Send chunk
  │     ├─ Wait for ChunkAck
  │     └─ Retry on failure
  └─ Send SurveyComplete

Server:
  ├─ Persist chunks to disk
  ├─ Track received chunks
  ├─ Compute SHA-256 after final chunk
  └─ Mark survey complete
```

**Failure handling:**

* If edge crashes → resume from last ACKed chunk
* If server crashes → resumes from persisted chunk map
* Duplicate chunks → ignored (idempotent)

---

### 1.5 Authentication & Integrity (Minimal but Real)

**Threat model (realistic):**

* Device theft
* Rogue relay
* Accidental misrouting
* Not nation-state adversaries

**Phase 1 security:**

* Pre-shared device key (per edge device)
* HMAC-SHA256 over payload
* TLS for 5G path
* Bluetooth link treated as untrusted

**Envelope extension:**

```protobuf
message Envelope {
  string device_id = 1;
  uint64 timestamp_us = 2;
  uint64 sequence = 3;
  MessageType type = 4;
  bytes payload = 5;
  bytes checksum = 6;
  bytes hmac = 7;      // HMAC(device_key, payload)
}
```

Server rejects:

* Unknown device_id
* Invalid HMAC
* Excessive clock skew (>5 min)
* Replayed sequence numbers

---

### 1.6 Observability (Non-negotiable in Production)

**Metrics exported (edge + relay):**

* messages_sent_total
* messages_failed_total
* current_transport (enum)
* queue_depth_messages
* queue_depth_bytes
* upload_throughput_bytes_per_sec
* reconnect_attempts

**Logging rules:**

* Structured logs (JSON)
* No `println!` in production
* Log every transport transition
* Log every retry escalation

**Tooling:**

* `tracing` + `tracing-subscriber`
* Optional Prometheus exporter
* Journald on device

---

## Risk Assessment

### Technical Risks

| Risk                        | Impact | Likelihood | Mitigation                               |
| --------------------------- | ------ | ---------- | ---------------------------------------- |
| 5G modem instability        | High   | Medium     | ModemManager watchdog, auto-reset        |
| Bluetooth pairing flakiness | Medium | High       | Pre-paired devices, static MAC allowlist |
| SD card wear-out            | High   | High       | External SSD only, no heavy writes to SD |
| Queue corruption            | High   | Low        | SQLite WAL mode + fsync                  |
| Power loss during upload    | Medium | High       | Chunk-level persistence                  |
| Thermal throttling          | Medium | Medium     | Active cooling, thermal monitoring       |
| Network flapping            | Medium | High       | FSM hysteresis + sticky transport        |

### Operational Risks

| Risk                  | Impact | Mitigation                          |
| --------------------- | ------ | ----------------------------------- |
| Field operator error  | Medium | Zero-touch startup, LEDs for state  |
| Misconfigured server  | High   | Health checks, startup validation   |
| Data growth over time | High   | Retention policies, cold storage    |
| Debugging in field    | Medium | SSH over relay, log snapshot export |

### Explicit Non-Risks (by design)

* ❌ “Realtime” uploads → not required
* ❌ Zero-latency → not realistic
* ❌ Perfect connectivity → assumed impossible
* ❌ Manual intervention → avoided

---

## Implementation Roadmap

### Phase 0 — Prototype (You are here)

**Goal:** Transport switching correctness

* [x] Rust workspace layout
* [x] Protobuf encoding
* [x] TCP-based 5G path
* [x] Simulated Bluetooth relay
* [x] FSM-based transport switching
* [ ] Run on real Pi 5 with wwan0

**Exit criteria:**

* Demonstrated failover in real hardware
* No crashes over 24-hour soak test

---

### Phase 1 — Field-Ready MVP

**Goal:** Deployable in real surveys

* [ ] Real Bluetooth (RFCOMM via BlueZ)
* [ ] ModemManager D-Bus integration
* [ ] Persistent SQLite queue
* [ ] Chunked file uploads
* [ ] Basic authentication (HMAC)
* [ ] Structured logging

**Timeline:** ~4–6 weeks
**Risk level:** Medium

---

### Phase 2 — Production Hardening

**Goal:** Reliable at scale

* [ ] TLS mutual auth
* [ ] Device provisioning flow
* [ ] Remote config updates
* [ ] Metrics dashboard
* [ ] Auto-recovery scripts
* [ ] OTA updates

**Timeline:** ~6–8 weeks
**Risk level:** Medium–High

---

### Phase 3 — Scale & Optimization

**Goal:** Handle many devices, big data

* [ ] Parallel chunk uploads
* [ ] Compression (zstd)
* [ ] Delta GPS batching
* [ ] Server-side ingest pipeline
* [ ] Object storage (S3/GCS)
* [ ] Indexing for geospatial queries

---

## Final Notes (Design Philosophy)

* This is **not** an IoT demo.
* This is a **field system** designed for bad networks.
* Every component assumes:

  * Power loss
  * Partial failure
  * Human error
  * Hardware quirks

If something can fail silently — it will.
So we log it.
If something can be retried — we retry it.
If something can be duplicated — we make it idempotent.

No magic.
Just engineering.

---