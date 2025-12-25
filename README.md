# EdgeLink

**EdgeLink** is a low-latency edge networking system for Raspberry Pi 5.

It sends data over a **5G USB modem when available** and **automatically falls back to Bluetooth** when the network is down. The same data path is used in both cases, with offline buffering and later sync.

Built for reliability, speed, and predictable behavior on Linux-based edge devices.

---

## What it does

* Uses **5G (USB modem)** as the primary transport
* Falls back to **Bluetooth** when the network is unavailable
* Buffers data locally when offline
* Syncs automatically when connectivity returns
* Runs on **Raspberry Pi 5**
* Written in **Rust** for low latency and no GC pauses

---

## Architecture (high level)

```
Data
  ↓
Core Logic
  ↓
Transport Selector
  ├── 5G Network
  └── Bluetooth
```

The application never cares how data is sent — only that it is sent.

---

## Repository layout

```
.
├── apps/
│   ├── edge-node/     # runs on Raspberry Pi (5G + Bluetooth)
│   ├── relay-node/    # receives data over Bluetooth
│   └── server/        # backend ingest
│
├── crates/
│   ├── core/          # business logic
│   ├── transport/     # 5G / Bluetooth implementations
│   ├── device/        # hardware access (modem, bluetooth)
│   └── protocol/      # protobuf message definitions
```

---

## Tech stack

* **Language**: Rust
* **Runtime**: Linux (aarch64)
* **Network**: USB 5G modem (QMI/MBIM via kernel drivers)
* **Fallback**: Bluetooth (BlueZ)
* **Protocol**: Protobuf

---

## Status

Early development.
Focused on correctness, observability, and predictable latency.

---