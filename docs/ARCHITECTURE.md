# Architecture

This document describes the system architecture of ResQTerra Engine.

## Overview

ResQTerra Engine is a distributed system with three components:

1. **Edge Device** - Runs on drone's companion computer
2. **Server** - Ground control station backend
3. **Relay Node** - Optional Bluetooth-to-5G bridge

```
┌─────────────────────────────────────────────────────────────────────┐
│                          GROUND STATION                             │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                         Server                                │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐    │  │
│  │  │   Session   │  │   Command   │  │   Telemetry Store   │    │  │
│  │  │   Manager   │  │  Dispatcher │  │                     │    │  │
│  │  └──────┬──────┘  └──────┬──────┘  └─────────────────────┘    │  │
│  │         │                │                                    │  │
│  │         └────────┬───────┘                                    │  │
│  │                  │                                            │  │
│  └──────────────────┼────────────────────────────────────────────┘  │
│                     │                                               │
└─────────────────────┼───────────────────────────────────────────────┘
                      │
        ┌─────────────┴─────────────┐
        │                           │
   TCP :8080                   TCP :9000
   (5G Direct)              (Bluetooth Relay)
        │                           │
        │                    ┌──────┴──────┐
        │                    │ Relay Node  │
        │                    │             │
        │                    │  BT ↔ TCP   │
        │                    └──────┬──────┘
        │                           │
        │                      Bluetooth
        │                           │
        └─────────────┬─────────────┘
                      │
┌─────────────────────┼───────────────────────────────────────────────┐
│                     │              DRONE                            │
│  ┌──────────────────┴───────────────────────────────────────────┐   │
│  │                      Edge Device                             │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │   │
│  │  │ Connection  │  │   Command   │  │   Safety Monitor    │   │   │
│  │  │   Manager   │  │  Executor   │  │   (State Machine)   │   │   │
│  │  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘   │   │
│  │         │                │                    │              │   │
│  │         └────────────────┼────────────────────┘              │   │
│  │                          │                                   │   │
│  │                   ┌──────┴──────┐                            │   │
│  │                   │   MAVLink   │                            │   │
│  │                   │   Bridge    │                            │   │
│  │                   └──────┬──────┘                            │   │
│  └──────────────────────────┼───────────────────────────────────┘   │
│                             │                                       │
│                        Serial/UDP                                   │
│                             │                                       │
│                    ┌────────┴─────────┐                             │
│                    │ Flight Controller│                             │
│                    │   (ArduPilot)    │                             │
│                    └──────────────────┘                             │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Edge Device

The edge device runs on a Raspberry Pi 5 mounted on the drone.

### Module Structure

```
src/
├── main.rs              # Application entry, event loop
├── connection/
│   ├── mod.rs
│   └── manager.rs       # Transport selection, failover
├── command/
│   ├── mod.rs
│   ├── executor.rs      # Command routing, ACK generation
│   └── handlers/        # Per-command-type handlers
│       ├── mission.rs
│       ├── rth.rs
│       ├── emergency.rs
│       ├── status.rs
│       └── config.rs
├── mavlink/
│   ├── mod.rs
│   ├── connection.rs    # FC connection (serial/UDP/TCP)
│   ├── commands.rs      # Command translation to MAVLink
│   └── telemetry.rs     # Telemetry parsing from MAVLink
├── safety/
│   ├── mod.rs
│   └── monitor.rs       # Connection monitoring, auto-RTH
└── transport/
    ├── mod.rs
    ├── five_g.rs        # TCP transport (placeholder)
    └── bluetooth.rs     # BT transport (placeholder)
```

### Connection Manager

Handles dual-transport connectivity with automatic failover.

```rust
pub struct ConnectionManager {
    config: ConnectionConfig,
    transport: CurrentTransport,
    event_tx: Sender<ConnectionEvent>,
}

pub enum ConnectionEvent {
    Connected { transport: String },
    Disconnected { reason: String },
    TransportSwitched { from: String, to: String },
    Received(Envelope),
}
```

**Failover Logic:**
1. Attempt 5G connection on startup
2. Monitor heartbeat responses
3. After 3 failed sends → switch to Bluetooth
4. Periodically probe 5G to restore primary

### Command Executor

Routes incoming commands to type-specific handlers.

```rust
pub struct CommandExecutor {
    device_id: String,
    sequence_id: Arc<AtomicU64>,
    state: Arc<RwLock<DroneState>>,
}

impl CommandExecutor {
    pub async fn execute(&self, cmd: &Command, header: &Header) -> Envelope {
        match CommandType::from(cmd.cmd_type) {
            CommandType::CmdMissionStart => handlers::handle_mission_start(...),
            CommandType::CmdRth => handlers::handle_rth(...),
            CommandType::CmdEmergencyStop => handlers::handle_emergency(...),
            // ...
        }
    }
}
```

### MAVLink Bridge

Translates ResQTerra commands to MAVLink and reads telemetry.

```
┌─────────────────────────────────────────────────────────┐
│                    MAVLink Bridge                       │
│                                                         │
│  ┌───────────────────┐   ┌─────────────────────────────┐│
│  │ MavCommandSender  │   │     TelemetryReader         ││
│  │                   │   │                             ││
│  │ • arm()           │   │ • process_message()         ││
│  │ • disarm()        │   │ • get_telemetry()           ││
│  │ • takeoff()       │   │ • get_position()            ││
│  │ • land()          │   │ • get_battery()             ││
│  │ • return_to_home()│   │ • is_armed()                ││
│  │ • start_mission() │   │ • get_mode()                ││
│  │ • abort_mission() │   │                             ││
│  │ • emergency_stop()│   │                             ││
│  │ • set_mode()      │   │                             ││
│  │ • goto_position() │   │                             ││
│  └────────┬──────────┘   └─────────────┬───────────────┘│
│           │                            │                │
│           └───────────┬────────────────┘                │
│                       │                                 │
│              ┌────────┴─────────┐                       │
│              │ FlightController │                       │
│              │   Connection     │                       │
│              │                  │                       │
│              │ • Serial         │                       │
│              │ • UDP            │                       │
│              │ • TCP            │                       │
│              └────────┬─────────┘                       │
└───────────────────────┼─────────────────────────────────┘
                        │
                   MAVLink v2
                        │
                ┌───────┴───────┐
                │   ArduPilot   │
                └───────────────┘
```

### Safety Monitor

Monitors system health and triggers protective actions.

```rust
pub struct SafetyMonitor {
    fsm: Arc<RwLock<SafetyStateMachine>>,
    last_server_heartbeat: Arc<RwLock<Instant>>,
    last_fc_heartbeat: Arc<RwLock<Instant>>,
    action_tx: Sender<SafetyAction>,
}

pub enum SafetyAction {
    ReturnToHome { reason: String },
    EmergencyStop { reason: String },
    StateChanged { from: DroneState, to: DroneState },
    None,
}
```

**Timeout Thresholds:**
- Heartbeat warning: 10 seconds
- Auto-RTH trigger: 30 seconds
- Emergency landing: 60 seconds

---

## Server

The server manages device sessions and dispatches commands.

### Module Structure

```
server/src/
├── main.rs              # TCP listener, accept loop
├── session/
│   ├── mod.rs
│   ├── manager.rs       # Device registry, lookup
│   └── connection.rs    # Per-device connection handler
└── command/
    ├── mod.rs
    ├── dispatcher.rs    # Command routing to devices
    └── timeout.rs       # Pending command tracking
```

### Session Manager

Tracks connected devices and their state.

```rust
pub struct SessionManager {
    devices: Arc<RwLock<HashMap<String, DeviceSession>>>,
}

pub struct DeviceSession {
    device_id: String,
    connected_at: Instant,
    last_heartbeat: Instant,
    transport: Transport,
    state: DroneState,
    sender: Sender<Envelope>,
}
```

### Command Dispatcher

Routes commands to devices with timeout tracking.

```rust
pub struct CommandDispatcher {
    pending: Arc<RwLock<HashMap<u64, PendingCommand>>>,
    timeout_ms: u64,
}

impl CommandDispatcher {
    pub async fn dispatch(&self, device_id: &str, cmd: Command) -> Result<()>;
    pub async fn handle_ack(&self, ack: &Ack);
    pub fn start_timeout_checker(&self) -> JoinHandle<()>;
}
```

---

## Shared Crate

Common definitions used by all components.

### Protobuf Messages

```protobuf
// Envelope wraps all messages
message Envelope {
    Header header = 1;
    oneof payload {
        Command command = 2;
        Telemetry telemetry = 3;
        Ack ack = 4;
        Heartbeat heartbeat = 5;
    }
}

// Command from server to edge
message Command {
    uint64 command_id = 1;
    CommandType cmd_type = 2;
    oneof params {
        MissionStart mission_start = 10;
        ReturnToHome rth = 11;
        ConfigUpdate config = 12;
    }
}

// Telemetry from edge to server
message Telemetry {
    GpsPosition position = 1;
    BatteryStatus battery = 2;
    DroneState state = 3;
    FlightControllerStatus fc_status = 4;
}
```

### Length-Prefix Codec

All messages use 4-byte big-endian length prefix:

```rust
pub struct FrameEncoder;
pub struct FrameDecoder;

impl Encoder<Envelope> for FrameEncoder {
    fn encode(&mut self, item: Envelope, dst: &mut BytesMut) -> Result<()> {
        let data = item.encode_to_vec();
        dst.put_u32(data.len() as u32);
        dst.put_slice(&data);
        Ok(())
    }
}
```

### Safety State Machine

Formal state machine for drone lifecycle:

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum DroneState {
    DroneIdle,
    DroneArmed,
    DroneTakingOff,
    DroneInMission,
    DroneReturningHome,
    DroneLanding,
    DroneEmergencyStopped,
    DroneError,
}

#[derive(Clone, Copy)]
pub enum SafetyEvent {
    Armed,
    TakeoffComplete,
    MissionStarted,
    RthTriggered,
    LandingInitiated,
    Landed,
    EmergencyStop,
    ErrorDetected,
    Reset,
}

impl SafetyStateMachine {
    pub fn transition(&mut self, event: SafetyEvent) -> Option<DroneState>;
}
```

---

## Data Flow

### Command Flow (Server → Drone)

```
Server                     Edge Device               Flight Controller
   │                            │                           │
   │ ─── Envelope(Command) ───→ │                           │
   │                            │                           │
   │                            │ ── parse command ──       │
   │                            │                           │
   │                            │ ─── MAVLink msg ────────→ │
   │                            │                           │
   │                            │ ←── MAVLink ACK ──────── │
   │                            │                           │
   │ ←── Envelope(Ack) ──────── │                           │
   │                            │                           │
```

### Telemetry Flow (Drone → Server)

```
Flight Controller          Edge Device                Server
   │                            │                        │
   │ ─── MAVLink telemetry ───→ │                        │
   │                            │                        │
   │                            │ ── parse & convert ──  │
   │                            │                        │
   │                            │ ─── Envelope(Telem) ──→│
   │                            │                        │
```

### Safety Timeout Flow

```
Edge Device (Safety Monitor)
   │
   ├── Check heartbeat timestamps
   │
   ├── If server_heartbeat > 30s stale:
   │      │
   │      ├── Emit SafetyAction::ReturnToHome
   │      │
   │      └── MAVLink: Set RTL mode
   │
   └── If server_heartbeat > 60s stale:
          │
          └── Emit SafetyAction::EmergencyStop
```

---

## Deployment Topology

### Development (Single Machine)

```
┌─────────────────────────────────────────┐
│              localhost                  │
│                                         │
│  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │  Server  │  │   Edge   │  │  SITL  │ │
│  │  :8080   │  │  Device  │  │ :14550 │ │
│  └────┬─────┘  └────┬─────┘  └───┬────┘ │
│       │             │            │      │
│       └─────────────┴────────────┘      │
│              TCP connections            │
└─────────────────────────────────────────┘
```

### Production (Field Deployment)

```
┌─────────────────┐         ┌─────────────────┐
│   Cloud/GCS     │         │    Vehicle      │
│                 │         │                 │
│  ┌───────────┐  │         │  ┌───────────┐  │
│  │  Server   │  │    5G   │  │   Relay   │  │
│  │           │←─┼─────────┼─→│   Node    │  │
│  └───────────┘  │         │  └─────┬─────┘  │
│                 │         │        │ BT     │
└─────────────────┘         └────────┼────────┘
                                     │
                            ┌────────┴────────┐
                            │     Drone       │
                            │                 │
                            │  ┌───────────┐  │
                            │  │   Edge    │  │
                            │  │  Device   │  │
                            │  └─────┬─────┘  │
                            │        │        │
                            │  ┌─────┴─────┐  │
                            │  │ ArduPilot │  │
                            │  └───────────┘  │
                            └─────────────────┘
```

---

## Error Handling

### Connection Errors

| Error | Handling |
|-------|----------|
| TCP connect failed | Retry with backoff, switch transport |
| Send timeout | Mark transport unhealthy, failover |
| Malformed message | Log error, drop message |
| Unknown message type | Log warning, ignore |

### MAVLink Errors

| Error | Handling |
|-------|----------|
| Connection lost | Auto-reconnect loop |
| Command rejected | Return error in ACK |
| No heartbeat | Safety monitor triggers RTH |

### Safety Errors

| Condition | Action |
|-----------|--------|
| Server heartbeat timeout | Auto-RTH |
| FC heartbeat timeout | Log error, continue monitoring |
| Low battery | Trigger RTH (via safety FSM) |
| Geofence breach | Trigger RTH |
