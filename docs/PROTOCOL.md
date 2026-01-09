# Protocol Specification

This document describes the wire protocol used for communication between ResQTerra components.

## Overview

All communication uses **Protocol Buffers** (protobuf) with **length-prefix framing** over TCP.

```
┌─────────────────────────────────────────────────────────────┐
│                      Wire Format                             │
├─────────────┬───────────────────────────────────────────────┤
│  Length     │              Protobuf Envelope                 │
│  (4 bytes)  │              (N bytes)                         │
│  Big-endian │                                                │
└─────────────┴───────────────────────────────────────────────┘
```

## Framing

### Length-Prefix Format

Every message is preceded by a 4-byte big-endian length prefix:

```
Offset  Size  Description
------  ----  -----------
0       4     Payload length (big-endian u32)
4       N     Protobuf-encoded Envelope
```

### Example

A 42-byte protobuf message:

```
00 00 00 2A  <-- Length: 42 (0x2A)
[42 bytes of protobuf data]
```

### Maximum Message Size

- Default limit: **10 MB** (10,485,760 bytes)
- Sensor data may use chunked transfer for larger payloads

### Rust Implementation

```rust
use bytes::{Buf, BufMut, BytesMut};
use prost::Message;

// Encoding
fn encode_frame<M: Message>(msg: &M, buf: &mut BytesMut) {
    let data = msg.encode_to_vec();
    buf.put_u32(data.len() as u32);
    buf.put_slice(&data);
}

// Decoding
fn decode_frame<M: Message + Default>(buf: &mut BytesMut) -> Option<M> {
    if buf.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() < 4 + len {
        return None;
    }
    buf.advance(4);
    let data = buf.split_to(len);
    M::decode(&data[..]).ok()
}
```

---

## Envelope Structure

All messages are wrapped in an `Envelope`:

```protobuf
message Envelope {
    Header header = 1;
    oneof payload {
        Telemetry telemetry = 2;
        Command command = 3;
        Ack ack = 4;
        Heartbeat heartbeat = 5;
        SensorData sensor_data = 6;
    }
}
```

### Header

Every envelope contains a header for routing and tracing:

```protobuf
message Header {
    string device_id = 1;      // Source identifier
    uint64 sequence_id = 2;    // Monotonic counter
    uint64 timestamp_ms = 3;   // Unix epoch milliseconds
    MessageType msg_type = 4;  // Payload discriminator
}
```

| Field | Description |
|-------|-------------|
| `device_id` | Unique identifier (e.g., "edge-001", "server-main") |
| `sequence_id` | Monotonically increasing, used for ACK matching |
| `timestamp_ms` | Message creation time (Unix epoch ms) |
| `msg_type` | Quick dispatch without parsing payload |

### Message Types

```protobuf
enum MessageType {
    MSG_UNKNOWN = 0;
    MSG_TELEMETRY = 1;
    MSG_COMMAND = 2;
    MSG_ACK = 3;
    MSG_HEARTBEAT = 4;
    MSG_SENSOR_DATA = 5;
}
```

---

## Message Types

### 1. Telemetry

**Direction**: Edge → Server

Periodic status updates from the drone.

```protobuf
message Telemetry {
    GpsPosition position = 1;
    BatteryStatus battery = 2;
    DroneState state = 3;
    FlightControllerStatus fc_status = 4;
    uint64 uptime_seconds = 5;
    ConnectionQuality conn_quality = 6;
}
```

#### GPS Position

```protobuf
message GpsPosition {
    double latitude = 1;         // Decimal degrees (-90 to 90)
    double longitude = 2;        // Decimal degrees (-180 to 180)
    float altitude_m = 3;        // Meters above sea level
    float heading_deg = 4;       // 0-360 degrees (0 = North)
    float ground_speed_mps = 5;  // Meters per second
    uint32 satellites = 6;       // Number of satellites
    float hdop = 7;              // Horizontal dilution of precision
}
```

#### Battery Status

```protobuf
message BatteryStatus {
    float voltage = 1;           // Volts
    float current = 2;           // Amps (positive = discharging)
    uint32 remaining_percent = 3;// 0-100
    uint32 remaining_seconds = 4;// Estimated flight time
}
```

#### Drone State

```protobuf
enum DroneState {
    DRONE_UNKNOWN = 0;
    DRONE_IDLE = 1;           // On ground, disarmed
    DRONE_PREFLIGHT = 2;      // Pre-flight checks
    DRONE_ARMED = 3;          // Armed, ready for takeoff
    DRONE_TAKING_OFF = 4;     // Ascending to altitude
    DRONE_IN_MISSION = 5;     // Executing mission
    DRONE_RETURNING_HOME = 6; // RTH in progress
    DRONE_LANDING = 7;        // Descent in progress
    DRONE_EMERGENCY = 8;      // Emergency stop triggered
}
```

### 2. Command

**Direction**: Server → Edge

Instructions sent to the drone.

```protobuf
message Command {
    uint64 command_id = 1;      // Unique identifier
    CommandType cmd_type = 2;   // Command discriminator
    uint64 expires_at_ms = 3;   // Expiry timestamp (0 = never)
    uint32 priority = 4;        // Higher = more urgent

    oneof params {
        MissionStart mission_start = 10;
        MissionAbort mission_abort = 11;
        ReturnToHome rth = 12;
        StatusRequest status_request = 13;
        ConfigUpdate config_update = 14;
        EmergencyStop emergency_stop = 15;
    }
}
```

#### Command Types

| Type | Value | Description |
|------|-------|-------------|
| `CMD_UNKNOWN` | 0 | Invalid/unset |
| `CMD_MISSION_START` | 1 | Start survey mission |
| `CMD_MISSION_ABORT` | 2 | Abort current mission |
| `CMD_RTH` | 3 | Return to home |
| `CMD_STATUS_REQUEST` | 4 | Request telemetry |
| `CMD_CONFIG_UPDATE` | 5 | Update configuration |
| `CMD_EMERGENCY_STOP` | 6 | Kill motors immediately |

#### Mission Start

```protobuf
message MissionStart {
    string mission_id = 1;           // Unique mission identifier
    SurveyArea survey_area = 2;      // Area to survey
    ScanPattern scan_pattern = 3;    // Pattern type
    float altitude_m = 4;            // Survey altitude
    float speed_mps = 5;             // Survey speed
    repeated SensorConfig sensors = 6;
}

message SurveyArea {
    repeated GpsCoordinate boundary = 1;  // Polygon vertices (CW order)
    GpsCoordinate home_position = 2;      // RTH destination
}

enum ScanPattern {
    PATTERN_UNKNOWN = 0;
    PATTERN_LAWNMOWER = 1;  // Parallel lines
    PATTERN_SPIRAL = 2;     // Inward spiral
    PATTERN_GRID = 3;       // Cross-hatch
    PATTERN_CUSTOM = 4;     // Custom waypoints
}
```

#### Return to Home

```protobuf
message ReturnToHome {
    float altitude_m = 1;  // RTH altitude (0 = FC default)
    float speed_mps = 2;   // RTH speed (0 = FC default)
}
```

#### Emergency Stop

```protobuf
message EmergencyStop {
    // No parameters
    // WARNING: Kills motors immediately - drone will fall!
}
```

### 3. Acknowledgment

**Direction**: Bidirectional

Confirms receipt and processing of messages.

```protobuf
message Ack {
    uint64 ack_sequence_id = 1;    // Sequence ID being acknowledged
    uint64 command_id = 2;         // Command ID (if acking a command)
    AckStatus status = 3;          // Result status
    string message = 4;            // Human-readable status/error
    uint64 processing_time_ms = 5; // Execution duration
}
```

#### ACK Status

| Status | Value | Meaning |
|--------|-------|---------|
| `ACK_UNKNOWN` | 0 | Invalid/unset |
| `ACK_RECEIVED` | 1 | Message received, processing |
| `ACK_ACCEPTED` | 2 | Command validated, will execute |
| `ACK_REJECTED` | 3 | Command rejected (see message) |
| `ACK_COMPLETED` | 4 | Execution finished successfully |
| `ACK_FAILED` | 5 | Execution failed (see message) |
| `ACK_EXPIRED` | 6 | Command expired before execution |

### 4. Heartbeat

**Direction**: Bidirectional

Connection keepalive and quick status.

```protobuf
message Heartbeat {
    uint64 uptime_ms = 1;         // System uptime
    DroneState state = 2;         // Current state
    uint32 pending_commands = 3;  // Queued commands
    bool healthy = 4;             // Overall health
}
```

**Timing:**
- Edge → Server: Every 5 seconds
- Server → Edge: Every 10 seconds
- Timeout threshold: 30 seconds (triggers safety RTH)

### 5. Sensor Data

**Direction**: Edge → Server

Bulk data from survey sensors.

```protobuf
message SensorData {
    string sensor_type = 1;          // "GPR", "LIDAR"
    string mission_id = 2;           // Associated mission
    uint64 capture_timestamp_ms = 3; // Capture time
    GpsPosition capture_position = 4;// Location
    bytes data = 5;                  // Raw payload
    string format = 6;               // "raw", "compressed"
    uint32 chunk_index = 7;          // Chunk number
    uint32 total_chunks = 8;         // Total chunks
}
```

**Chunking:**
- Default chunk size: 1 MB
- Chunks are numbered 0 to N-1
- Each chunk ACKed individually
- Resume from last ACKed chunk on reconnect

---

## Connection Flow

### Initial Connection

```
Edge                                    Server
  │                                        │
  │ ──────── TCP Connect ─────────────────→│
  │                                        │
  │ ──────── Heartbeat (healthy=true) ───→ │
  │                                        │
  │ ←─────── Heartbeat (healthy=true) ──── │
  │                                        │
  │ ←─────── Command (STATUS_REQUEST) ──── │
  │                                        │
  │ ──────── Telemetry ──────────────────→ │
  │                                        │
  │ ──────── Ack (COMPLETED) ────────────→ │
  │                                        │
```

### Command Execution

```
Server                                  Edge
  │                                        │
  │ ──────── Command (MISSION_START) ────→ │
  │                                        │
  │ ←─────── Ack (RECEIVED) ─────────────  │
  │                                        │
  │             [Mission executes]          │
  │                                        │
  │ ←─────── Ack (COMPLETED) ────────────  │
  │                                        │
```

### Error Handling

```
Server                                  Edge
  │                                        │
  │ ──────── Command (invalid params) ───→ │
  │                                        │
  │ ←─────── Ack (REJECTED, "...") ──────  │
  │                                        │
```

---

## Transport Details

### TCP Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 8080 | TCP | Server ↔ Edge (5G) |
| 9000 | TCP | Relay ↔ Edge (simulated BT) |

### Connection Handling

- **Reconnect**: Automatic with exponential backoff (1s, 2s, 4s, max 30s)
- **Keepalive**: TCP keepalive enabled (60s interval)
- **Timeout**: Read timeout 60 seconds

### Transport Selection

The edge device selects transport based on availability:

1. **5G Primary**: Connect to server:8080 directly
2. **Bluetooth Fallback**: Connect to relay:9000 if 5G unavailable
3. **Offline**: Buffer messages locally

---

## Sequence Numbers

### Purpose

- Match ACKs to requests
- Detect message loss
- Order messages in logs

### Generation

- Monotonically increasing per sender
- Reset on process restart (acceptable for this use case)
- 64-bit unsigned integer

### ACK Matching

```rust
// Server sends command
let cmd_seq = next_sequence_id();  // e.g., 1001
send(Command { sequence_id: cmd_seq, ... });

// Edge receives and ACKs
recv(Command { sequence_id: 1001, ... });
send(Ack { ack_sequence_id: 1001, status: RECEIVED });
```

---

## Error Codes

### ACK Messages

| Status | When Used |
|--------|-----------|
| `REJECTED` | Invalid parameters, unauthorized, state conflict |
| `FAILED` | Execution error (e.g., MAVLink command failed) |
| `EXPIRED` | Command `expires_at_ms` passed before execution |

### Common Rejection Reasons

| Message | Cause |
|---------|-------|
| "Invalid mission_id" | Empty or malformed mission ID |
| "No survey area" | Missing boundary polygon |
| "Drone not armed" | Cannot start mission without arming |
| "Already in mission" | Mission in progress |
| "Emergency stop active" | Drone in emergency state |

---

## Security Considerations

### Current State (Development)

- No authentication
- No encryption
- Trust all connections

### Production Requirements

- TLS for 5G transport
- Pre-shared device keys
- HMAC message signing
- Replay protection (sequence + timestamp)

---

## Compatibility

### Versioning

- Protobuf fields use explicit numbers
- New optional fields can be added
- Never reuse or renumber fields
- Use `reserved` for removed fields

### Forward Compatibility

Unknown fields are preserved and forwarded (default protobuf behavior).

### Backward Compatibility

- All fields optional (protobuf3)
- Check field presence before use
- Use sensible defaults for missing fields
