//! ResQTerra Relay Node
//!
//! Accepts connections from edge devices (via TCP or Bluetooth RFCOMM)
//! and forwards them to the ground control server.

use anyhow::Result;
use bluer::rfcomm::{Listener as RfcommListener, SocketAddr as RfcommAddr, Stream as RfcommStream};
use futures::StreamExt;
use std::env;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Default RFCOMM channel for ResQTerra relay service
const DEFAULT_RFCOMM_CHANNEL: u8 = 1;

/// Server address to forward to
const DEFAULT_SERVER: &str = "127.0.0.1:8080";

/// TCP listen address
const DEFAULT_TCP_LISTEN: &str = "0.0.0.0:9000";

/// Relay configuration
struct RelayConfig {
    /// Server address to forward to
    server_addr: String,
    /// TCP listen address (for development)
    tcp_listen: String,
    /// RFCOMM channel
    rfcomm_channel: u8,
    /// Enable real Bluetooth RFCOMM
    enable_rfcomm: bool,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            server_addr: DEFAULT_SERVER.into(),
            tcp_listen: DEFAULT_TCP_LISTEN.into(),
            rfcomm_channel: DEFAULT_RFCOMM_CHANNEL,
            enable_rfcomm: false,
        }
    }
}

impl RelayConfig {
    fn from_env() -> Self {
        Self {
            server_addr: env::var("RELAY_SERVER").unwrap_or_else(|_| DEFAULT_SERVER.into()),
            tcp_listen: env::var("RELAY_TCP_LISTEN").unwrap_or_else(|_| DEFAULT_TCP_LISTEN.into()),
            rfcomm_channel: env::var("RELAY_RFCOMM_CHANNEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_RFCOMM_CHANNEL),
            enable_rfcomm: env::var("RELAY_ENABLE_RFCOMM")
                .map(|v| v == "1" || v.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = RelayConfig::from_env();

    println!("ResQTerra Relay Node");
    println!("  Server: {}", config.server_addr);
    println!("  TCP listen: {}", config.tcp_listen);
    println!("  RFCOMM enabled: {}", config.enable_rfcomm);

    // Start TCP listener
    let tcp_listener = TcpListener::bind(&config.tcp_listen).await?;
    println!("TCP relay listening on {}", config.tcp_listen);

    // Start RFCOMM listener if enabled
    let rfcomm_task = if config.enable_rfcomm {
        println!("Starting RFCOMM listener on channel {}", config.rfcomm_channel);
        let server_addr = config.server_addr.clone();
        let channel = config.rfcomm_channel;
        Some(tokio::spawn(async move {
            if let Err(e) = run_rfcomm_listener(channel, &server_addr).await {
                eprintln!("[RFCOMM] Listener error: {}", e);
            }
        }))
    } else {
        None
    };

    // Main TCP accept loop
    let server_addr = config.server_addr.clone();
    loop {
        match tcp_listener.accept().await {
            Ok((socket, addr)) => {
                println!("[TCP] Connection from {}", addr);
                let server = server_addr.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(socket, &server).await {
                        eprintln!("[TCP] Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("[TCP] Accept error: {}", e);
            }
        }
    }
}

/// Run the RFCOMM Bluetooth listener
async fn run_rfcomm_listener(channel: u8, server_addr: &str) -> Result<()> {
    let addr = RfcommAddr::new(bluer::Address::any(), channel);
    let listener = RfcommListener::bind(addr).await?;

    let local_addr = listener.as_ref().local_addr()?;
    println!("[RFCOMM] Listening on channel {}", local_addr.channel);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("[RFCOMM] Connection from {}", addr);
                let server = server_addr.to_string();
                tokio::spawn(async move {
                    if let Err(e) = handle_rfcomm_connection(stream, &server).await {
                        eprintln!("[RFCOMM] Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("[RFCOMM] Accept error: {}", e);
            }
        }
    }
}

/// Handle a TCP connection from edge device
async fn handle_connection(mut edge: TcpStream, server_addr: &str) -> Result<()> {
    let mut server = TcpStream::connect(server_addr).await?;
    println!("[TCP] Connected to server {}", server_addr);

    // Bidirectional forwarding
    let (mut edge_read, mut edge_write) = edge.split();
    let (mut server_read, mut server_write) = server.split();

    let edge_to_server = async {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = edge_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            server_write.write_all(&buf[..n]).await?;
        }
        Ok::<_, anyhow::Error>(())
    };

    let server_to_edge = async {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = server_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            edge_write.write_all(&buf[..n]).await?;
        }
        Ok::<_, anyhow::Error>(())
    };

    tokio::select! {
        r = edge_to_server => r?,
        r = server_to_edge => r?,
    }

    println!("[TCP] Connection closed");
    Ok(())
}

/// Handle an RFCOMM connection from edge device
async fn handle_rfcomm_connection(mut edge: RfcommStream, server_addr: &str) -> Result<()> {
    let mut server = TcpStream::connect(server_addr).await?;
    println!("[RFCOMM] Connected to server {}", server_addr);

    // Bidirectional forwarding
    let (mut edge_read, mut edge_write) = edge.split();
    let (mut server_read, mut server_write) = server.split();

    let edge_to_server = async {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = edge_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            server_write.write_all(&buf[..n]).await?;
        }
        Ok::<_, anyhow::Error>(())
    };

    let server_to_edge = async {
        let mut buf = vec![0u8; 4096];
        loop {
            let n = server_read.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            edge_write.write_all(&buf[..n]).await?;
        }
        Ok::<_, anyhow::Error>(())
    };

    tokio::select! {
        r = edge_to_server => r?,
        r = server_to_edge => r?,
    }

    println!("[RFCOMM] Connection closed");
    Ok(())
}
