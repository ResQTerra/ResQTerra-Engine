# Deployment Guide

This guide covers building, configuring, and deploying ResQTerra Engine components.

## Prerequisites

### Development Machine

- **Rust**: 1.75+ (`rustup update stable`)
- **Protobuf Compiler**: `protoc` 3.x+
- **Build Tools**: `build-essential` (Linux) or Xcode (macOS)

```bash
# Ubuntu/Debian
sudo apt update
sudo apt install -y build-essential protobuf-compiler

# macOS
brew install protobuf

# Verify
rustc --version
protoc --version
```

### Target Hardware

| Component | Hardware | OS |
|-----------|----------|-----|
| Edge Device | Raspberry Pi 5 (8GB) | Raspberry Pi OS 64-bit |
| Relay Node | Raspberry Pi 5 / Laptop | Linux / macOS |
| Server | Cloud VM / Local machine | Linux |

---

## Building

### Development Build

```bash
# Clone repository
git clone https://github.com/your-org/ResQTerra-Engine.git
cd ResQTerra-Engine

# Build all components
cargo build

# Run tests
cargo test
```

### Release Build

```bash
# Optimized build
cargo build --release

# Binaries located at:
# - target/release/edge-device
# - target/release/server
# - target/release/relay-node
```

### Cross-Compilation (for Raspberry Pi)

```bash
# Install cross-compilation target
rustup target add aarch64-unknown-linux-gnu

# Install linker (Ubuntu/Debian)
sudo apt install gcc-aarch64-linux-gnu

# Create .cargo/config.toml
mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
EOF

# Build for ARM64
cargo build --release --target aarch64-unknown-linux-gnu

# Binary at: target/aarch64-unknown-linux-gnu/release/edge-device
```

---

## Configuration

### Edge Device

Configuration is currently hardcoded in `src/main.rs`. Key settings:

```rust
let config = ConnectionConfig {
    device_id: "edge-001".into(),
    server_5g: "127.0.0.1:8080".into(),   // 5G server address
    server_bt: "127.0.0.1:9000".into(),   // Bluetooth relay address
    ..Default::default()
};

let fc_config = FcConfig {
    connection: FcConnectionType::Udp {
        address: "127.0.0.1:14550".into(), // Flight controller
    },
    ..Default::default()
};
```

#### Environment Variables (Future)

```bash
# Planned configuration via environment
export RESQTERRA_DEVICE_ID="edge-001"
export RESQTERRA_SERVER_5G="10.0.0.100:8080"
export RESQTERRA_SERVER_BT="10.0.0.50:9000"
export RESQTERRA_FC_CONNECTION="udp:127.0.0.1:14550"
```

### Server

Server listens on port 8080 by default:

```rust
// server/src/main.rs
let listener = TcpListener::bind("0.0.0.0:8080").await?;
```

### Relay Node

Relay listens on port 9000 and forwards to server:

```rust
// relay-node/src/main.rs
let listen_addr = "0.0.0.0:9000";
let server_addr = "127.0.0.1:8080";
```

---

## Running

### Development (Single Machine)

Open three terminals:

```bash
# Terminal 1: Server
cargo run -p server

# Terminal 2: Edge Device
cargo run -p edge-device

# Terminal 3 (optional): Relay Node
cargo run -p relay-node
```

### With ArduPilot SITL

```bash
# Terminal 1: Start SITL (requires ArduPilot installation)
cd ~/ardupilot
./Tools/autotest/sim_vehicle.py -v ArduCopter --console --map

# Terminal 2: Server
cargo run -p server

# Terminal 3: Edge Device (connects to SITL on UDP 14550)
cargo run -p edge-device
```

### Production Deployment

#### Server (Cloud VM)

```bash
# Copy binary
scp target/release/server user@server:/opt/resqterra/

# SSH to server
ssh user@server

# Run with systemd (see below) or directly
/opt/resqterra/server
```

#### Edge Device (Raspberry Pi)

```bash
# Copy binary (cross-compiled)
scp target/aarch64-unknown-linux-gnu/release/edge-device pi@drone:/opt/resqterra/

# SSH to Pi
ssh pi@drone

# Run
/opt/resqterra/edge-device
```

---

## Systemd Services

### Server Service

Create `/etc/systemd/system/resqterra-server.service`:

```ini
[Unit]
Description=ResQTerra Ground Control Server
After=network.target

[Service]
Type=simple
User=resqterra
ExecStart=/opt/resqterra/server
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable resqterra-server
sudo systemctl start resqterra-server
sudo systemctl status resqterra-server
```

### Edge Device Service

Create `/etc/systemd/system/resqterra-edge.service`:

```ini
[Unit]
Description=ResQTerra Edge Device
After=network.target

[Service]
Type=simple
User=root
ExecStart=/opt/resqterra/edge-device
Restart=always
RestartSec=5
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable resqterra-edge
sudo systemctl start resqterra-edge
sudo journalctl -u resqterra-edge -f
```

---

## Flight Controller Setup

### ArduPilot SITL (Simulation)

```bash
# Install ArduPilot
git clone https://github.com/ArduPilot/ardupilot.git
cd ardupilot
git submodule update --init --recursive
./Tools/environment_install/install-prereqs-ubuntu.sh -y

# Run SITL
./Tools/autotest/sim_vehicle.py -v ArduCopter \
    --console \
    --map \
    --out=udp:127.0.0.1:14550
```

The edge device connects to `127.0.0.1:14550` by default.

### Real Flight Controller

#### Serial Connection (USB)

```rust
let fc_config = FcConfig {
    connection: FcConnectionType::Serial {
        port: "/dev/ttyACM0".into(),  // USB
        baud: 115200,
    },
    ..Default::default()
};
```

#### Serial Connection (UART - GPIO)

```rust
let fc_config = FcConfig {
    connection: FcConnectionType::Serial {
        port: "/dev/serial0".into(),  // GPIO UART
        baud: 921600,
    },
    ..Default::default()
};
```

Enable UART on Raspberry Pi:
```bash
# /boot/config.txt
enable_uart=1
dtoverlay=disable-bt
```

#### UDP Connection (WiFi/Ethernet)

```rust
let fc_config = FcConfig {
    connection: FcConnectionType::Udp {
        address: "192.168.1.100:14550".into(),
    },
    ..Default::default()
};
```

---

## Network Configuration

### Firewall Rules

#### Server

```bash
# Allow incoming on port 8080
sudo ufw allow 8080/tcp
```

#### Edge Device

```bash
# No incoming ports needed (outbound only)
# Ensure outbound TCP is allowed
```

### 5G Modem Setup (Future)

```bash
# Install ModemManager
sudo apt install modemmanager

# Check modem status
mmcli -L
mmcli -m 0

# Connect
mmcli -m 0 --simple-connect="apn=your.apn.here"

# Check connection
ip addr show wwan0
```

---

## Monitoring

### Logs

```bash
# Server logs
journalctl -u resqterra-server -f

# Edge device logs
journalctl -u resqterra-edge -f

# Filter by level
journalctl -u resqterra-edge -p warning
```

### Health Checks

```bash
# Check server is listening
nc -zv localhost 8080

# Check processes
ps aux | grep resqterra

# Check systemd status
systemctl status resqterra-server
systemctl status resqterra-edge
```

---

## Troubleshooting

### Connection Issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| "Connection refused" | Server not running | Start server first |
| "Connection timeout" | Firewall blocking | Check firewall rules |
| "Transport switched" | 5G unavailable | Check network/modem |

### MAVLink Issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| "Failed to connect" | Wrong port/address | Check FC connection config |
| "No heartbeat" | FC not responding | Check FC is powered and configured |
| "Permission denied" | Serial port access | Add user to `dialout` group |

```bash
# Fix serial port permissions
sudo usermod -a -G dialout $USER
# Log out and back in
```

### Build Issues

| Error | Cause | Solution |
|-------|-------|----------|
| "protoc not found" | Missing protobuf | Install `protobuf-compiler` |
| "linker not found" | Missing cross-compiler | Install `gcc-aarch64-linux-gnu` |
| "OUT_DIR not set" | Build script issue | Run `cargo clean && cargo build` |

---

## Security Considerations

### Current State (Development)

- No TLS encryption
- No authentication
- Plaintext communication

### Production Recommendations

1. **TLS**: Use TLS for all TCP connections
2. **Authentication**: Implement device certificates or API keys
3. **Firewall**: Restrict server access to known IPs
4. **Updates**: Implement secure OTA updates

---

## Directory Structure (Deployed)

```
/opt/resqterra/
├── edge-device          # Edge device binary
├── server               # Server binary
├── relay-node           # Relay binary
├── config/              # Configuration files (future)
│   └── edge.toml
└── logs/                # Log files (if not using journald)
    └── edge.log
```

---

## Quick Reference

### Start Everything (Development)

```bash
# One-liner with tmux
tmux new-session -d -s resqterra 'cargo run -p server' \; \
     split-window -h 'cargo run -p edge-device' \; \
     attach
```

### Check Status

```bash
# All services
systemctl status 'resqterra-*'

# Logs
journalctl -u 'resqterra-*' --since '1 hour ago'
```

### Restart Services

```bash
sudo systemctl restart resqterra-server
sudo systemctl restart resqterra-edge
```
