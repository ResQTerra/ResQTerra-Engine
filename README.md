# ResQTerra Engine

**ResQTerra Engine** is an edge computing system for autonomous drone operations with reliable connectivity failover.

Built for search-and-rescue and survey missions where network reliability cannot be guaranteed. The drone edge device connects to a ground control server via **5G** and automatically falls back to **Bluetooth relay** when the network is unavailable.

---

## Features

- **Dual Transport Failover**: 5G primary, Bluetooth relay fallback
- **MAVLink Integration**: ArduPilot/PX4 flight controller support
- **Safety State Machine**: Automatic RTH on connection loss
- **Command Infrastructure**: Mission control, emergency stop, RTH
- **Protobuf Protocol**: Efficient binary messaging with length-prefix framing
- **Telemetry Pipeline**: GPS, battery, flight mode streaming

---

## Architecture

```
                         ┌─────────────────┐
                         │  Ground Server  │
                         │    (Control)    │
                         └────────┬────────┘
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
               5G Primary              Bluetooth Fallback
                    │                           │
                    │                    ┌──────┴──────┐
                    │                    │ Relay Node  │
                    │                    │  (Vehicle)  │
                    │                    └──────┬──────┘
                    │                           │
                    └─────────────┬─────────────┘
                                  │
                         ┌────────┴────────┐
                         │  Edge Device    │
                         │  (Raspberry Pi) │
                         └────────┬────────┘
                                  │
                              MAVLink
                                  │
                         ┌────────┴────────┐
                         │ Flight Controller│
                         │   (ArduPilot)   │
                         └─────────────────┘
```

---

## Repository Structure

```
ResQTerra-Engine/
├── src/                    # Edge device (runs on drone)
│   ├── main.rs
│   ├── command/            # Command executor and handlers
│   ├── connection/         # Transport manager (5G/BT failover)
│   ├── mavlink/            # Flight controller bridge
│   ├── safety/             # Safety state machine
│   └── transport/          # Transport implementations
│
├── server/                 # Ground control server
│   └── src/
│       ├── command/        # Command dispatcher
│       └── session/        # Device session management
│
├── relay-node/             # Bluetooth relay (optional)
│   └── src/main.rs
│
├── shared/                 # Shared protocol crate
│   ├── proto/              # Protobuf definitions
│   │   └── resqterra.proto
│   └── src/
│       ├── codec.rs        # Length-prefix framing
│       ├── state_machine.rs # Safety FSM
│       └── lib.rs          # Generated protobuf + exports
│
└── docs/                   # Documentation
    ├── ARCHITECTURE.md
    ├── PROTOCOL.md
    └── DEPLOYMENT.md
```

---

## Components

### Edge Device (`src/`)

Runs on the drone's companion computer (Raspberry Pi 5).

| Module | Purpose |
|--------|---------|
| `connection/` | Manages 5G/Bluetooth transport with automatic failover |
| `command/` | Executes commands from server (mission, RTH, emergency) |
| `mavlink/` | Bridges to ArduPilot via MAVLink protocol |
| `safety/` | Monitors connection health, triggers auto-RTH |

### Server (`server/`)

Ground control station backend.

| Module | Purpose |
|--------|---------|
| `session/` | Manages device connections and state |
| `command/` | Dispatches commands with timeout tracking |

### Shared (`shared/`)

Common protocol definitions used by all components.

- Protobuf messages (Envelope, Command, Telemetry, etc.)
- Length-prefix codec for framing
- Safety state machine (DroneState transitions)

---

## Quick Start

### Prerequisites

- Rust 1.75+
- Protobuf compiler (`protoc`)

### Build

```bash
# Build all workspace members
cargo build

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

### Run (Development)

```bash
# Terminal 1: Start server
cargo run -p server

# Terminal 2: Start edge device
cargo run -p edge-device

# Optional - Terminal 3: Start relay
cargo run -p relay-node
```

### Configuration

Edge device connects to:
- **5G Server**: `127.0.0.1:8080`
- **BT Relay**: `127.0.0.1:9000`
- **Flight Controller**: UDP `127.0.0.1:14550` (SITL default)

---

## Protocol Overview

All messages use Protobuf with length-prefix framing:

```
┌─────────────┬───────────────────┐
│ Length (4B) │ Protobuf Envelope │
└─────────────┴───────────────────┘
```

### Message Types

| Type | Direction | Purpose |
|------|-----------|---------|
| `Command` | Server → Edge | Mission control commands |
| `Ack` | Edge → Server | Command acknowledgment |
| `Telemetry` | Edge → Server | Position, battery, state |
| `Heartbeat` | Bidirectional | Connection health |

### Commands

| Command | Description |
|---------|-------------|
| `CMD_MISSION_START` | Start survey mission with waypoints |
| `CMD_MISSION_ABORT` | Abort mission, hold position |
| `CMD_RTH` | Return to home/launch |
| `CMD_EMERGENCY_STOP` | Kill motors immediately |
| `CMD_STATUS_REQUEST` | Request telemetry update |

---

## Safety Features

### Connection Loss Handling

1. **Heartbeat timeout** (10s) → Warning state
2. **Extended timeout** (30s) → Automatic RTH
3. **Critical timeout** (60s) → Emergency landing

### State Machine

```
IDLE → ARMED → TAKING_OFF → IN_MISSION
                    ↓
              RETURNING_HOME → LANDING → IDLE
                    ↓
              EMERGENCY_STOP
```

---

## MAVLink Integration

Supports ArduPilot Copter via MAVLink 2.0:

### Connection Types
- **Serial**: `/dev/ttyACM0` (USB) or `/dev/serial0` (UART)
- **UDP**: `127.0.0.1:14550` (SITL, MAVProxy)
- **TCP**: `127.0.0.1:5760`

### Supported Commands
- Arm/Disarm
- Takeoff/Land
- RTL (Return to Launch)
- Guided waypoint navigation
- Mission upload and start
- Emergency motor kill

### Telemetry
- GPS position (lat, lon, alt, heading)
- Battery voltage, current, remaining
- Flight mode and armed state
- Status text and fault messages

---

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust |
| Async Runtime | Tokio |
| Serialization | Protobuf (prost) |
| MAVLink | mavlink-rs (ardupilotmega) |
| Target Platform | Raspberry Pi 5 (aarch64) |

---

## Documentation

- [Architecture](docs/ARCHITECTURE.md) - System design and components
- [Protocol](docs/PROTOCOL.md) - Wire protocol specification
- [Deployment](docs/DEPLOYMENT.md) - Installation and configuration
- [API Docs](target/doc/edge_device/index.html) - Generated Rust docs

---

## Status

**Current Phase**: Core infrastructure complete

- [x] Phase 1: Shared Protocol Crate
- [x] Phase 2: Persistent Connection Layer
- [x] Phase 3: Command Infrastructure
- [x] Phase 4: Safety State Machine
- [x] Phase 5: MAVLink Bridge
- [ ] Phase 6: Real Bluetooth Transport
- [ ] Phase 7: Production Hardening

---

## License

MIT
