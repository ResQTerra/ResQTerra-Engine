use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub struct SensorPacket {
    #[prost(string, tag = "1")]
    pub device_id: String,

    #[prost(uint64, tag = "2")]
    pub timestamp: u64,

    #[prost(string, tag = "3")]
    pub payload: String,
}

pub fn encode(packet: &SensorPacket) -> Vec<u8> {
    let mut buf = Vec::new();
    packet.encode(&mut buf).unwrap();
    buf
}

pub fn decode(buf: &[u8]) -> SensorPacket {
    SensorPacket::decode(buf).unwrap()
}
