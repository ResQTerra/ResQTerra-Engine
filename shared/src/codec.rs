//! Length-prefixed codec for TCP framing
//!
//! All messages are framed as:
//! ```text
//! [ 4 bytes: length (u32, big-endian) ][ N bytes: protobuf Envelope ]
//! ```
//!
//! This ensures message boundaries are preserved over TCP streams.

use bytes::{Buf, BufMut, Bytes, BytesMut};
use prost::Message;
use thiserror::Error;

use crate::Envelope;

/// Maximum message size (10 MB) to prevent memory exhaustion
pub const MAX_MESSAGE_SIZE: u32 = 10 * 1024 * 1024;

/// Errors that can occur during encoding/decoding
#[derive(Error, Debug)]
pub enum CodecError {
    #[error("Message too large: {0} bytes (max: {MAX_MESSAGE_SIZE})")]
    MessageTooLarge(usize),

    #[error("Invalid message length prefix: {0}")]
    InvalidLength(u32),

    #[error("Not enough data: need {needed} bytes, have {available}")]
    NotEnoughData { needed: usize, available: usize },

    #[error("Protobuf decode error: {0}")]
    DecodeError(#[from] prost::DecodeError),

    #[error("Protobuf encode error: {0}")]
    EncodeError(#[from] prost::EncodeError),
}

/// Encode an Envelope into a length-prefixed byte buffer
pub fn encode(envelope: &Envelope) -> Result<Bytes, CodecError> {
    let msg_len = envelope.encoded_len();

    if msg_len > MAX_MESSAGE_SIZE as usize {
        return Err(CodecError::MessageTooLarge(msg_len));
    }

    // 4 bytes for length prefix + message bytes
    let mut buf = BytesMut::with_capacity(4 + msg_len);

    // Write length prefix (big-endian u32)
    buf.put_u32(msg_len as u32);

    // Write protobuf message
    envelope.encode(&mut buf)?;

    Ok(buf.freeze())
}

/// Encode an Envelope directly into a provided buffer
pub fn encode_into(envelope: &Envelope, buf: &mut BytesMut) -> Result<(), CodecError> {
    let msg_len = envelope.encoded_len();

    if msg_len > MAX_MESSAGE_SIZE as usize {
        return Err(CodecError::MessageTooLarge(msg_len));
    }

    // Reserve space
    buf.reserve(4 + msg_len);

    // Write length prefix (big-endian u32)
    buf.put_u32(msg_len as u32);

    // Write protobuf message
    envelope.encode(buf)?;

    Ok(())
}

/// Try to decode a length-prefixed Envelope from a buffer
///
/// Returns:
/// - `Ok(Some(envelope))` if a complete message was decoded
/// - `Ok(None)` if more data is needed
/// - `Err(...)` if the data is invalid
pub fn decode(buf: &mut BytesMut) -> Result<Option<Envelope>, CodecError> {
    // Need at least 4 bytes for the length prefix
    if buf.len() < 4 {
        return Ok(None);
    }

    // Peek at the length prefix without consuming
    let msg_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);

    // Validate length
    if msg_len > MAX_MESSAGE_SIZE {
        return Err(CodecError::InvalidLength(msg_len));
    }

    let total_len = 4 + msg_len as usize;

    // Check if we have the complete message
    if buf.len() < total_len {
        return Ok(None);
    }

    // Consume the length prefix
    buf.advance(4);

    // Split off the message bytes
    let msg_bytes = buf.split_to(msg_len as usize);

    // Decode the protobuf message
    let envelope = Envelope::decode(msg_bytes)?;

    Ok(Some(envelope))
}

/// Decoder state machine for streaming decoding
#[derive(Debug, Default)]
pub struct FrameDecoder {
    /// Partial frame data being accumulated
    buffer: BytesMut,
}

impl FrameDecoder {
    /// Create a new frame decoder
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Add data to the decoder buffer
    pub fn extend(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    /// Try to decode the next frame from the buffer
    ///
    /// Call this repeatedly until it returns `Ok(None)` to drain all complete frames
    pub fn decode_next(&mut self) -> Result<Option<Envelope>, CodecError> {
        decode(&mut self.buffer)
    }

    /// Get the current buffer length (for debugging)
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

/// Encoder for building frames
#[derive(Debug, Default)]
pub struct FrameEncoder {
    /// Output buffer
    buffer: BytesMut,
}

impl FrameEncoder {
    /// Create a new frame encoder
    pub fn new() -> Self {
        Self {
            buffer: BytesMut::with_capacity(4096),
        }
    }

    /// Encode an envelope and add to the output buffer
    pub fn encode(&mut self, envelope: &Envelope) -> Result<(), CodecError> {
        encode_into(envelope, &mut self.buffer)
    }

    /// Take the encoded bytes, leaving an empty buffer
    pub fn take(&mut self) -> Bytes {
        self.buffer.split().freeze()
    }

    /// Check if the encoder has any pending data
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Header, Heartbeat, MessageType};

    fn create_test_envelope() -> Envelope {
        Envelope {
            header: Some(Header::new("test-device", MessageType::MsgHeartbeat, 1)),
            payload: Some(crate::envelope::Payload::Heartbeat(Heartbeat::new(
                1000,
                crate::DroneState::DroneIdle,
                0,
                true,
            ))),
        }
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = create_test_envelope();

        // Encode
        let encoded = encode(&original).expect("encode failed");

        // Verify length prefix
        let len_prefix = u32::from_be_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]);
        assert_eq!(len_prefix as usize, encoded.len() - 4);

        // Decode
        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = decode(&mut buf).expect("decode failed").expect("no message");

        // Verify
        assert_eq!(
            decoded.header.as_ref().unwrap().device_id,
            original.header.as_ref().unwrap().device_id
        );
        assert!(buf.is_empty(), "buffer should be empty after decode");
    }

    #[test]
    fn test_partial_decode() {
        let envelope = create_test_envelope();
        let encoded = encode(&envelope).expect("encode failed");

        // Try decoding with only partial data
        let mut buf = BytesMut::from(&encoded[..5]); // Only 5 bytes
        let result = decode(&mut buf).expect("decode should not fail on partial data");
        assert!(result.is_none(), "should return None for partial data");

        // Buffer should be unchanged (data not consumed)
        assert_eq!(buf.len(), 5);
    }

    #[test]
    fn test_frame_decoder() {
        let envelope = create_test_envelope();
        let encoded = encode(&envelope).expect("encode failed");

        let mut decoder = FrameDecoder::new();

        // Feed data in chunks
        decoder.extend(&encoded[..5]);
        assert!(decoder.decode_next().expect("decode error").is_none());

        decoder.extend(&encoded[5..]);
        let decoded = decoder
            .decode_next()
            .expect("decode error")
            .expect("should have message");

        assert_eq!(
            decoded.header.as_ref().unwrap().device_id,
            envelope.header.as_ref().unwrap().device_id
        );
    }

    #[test]
    fn test_multiple_frames() {
        let envelope1 = create_test_envelope();
        let envelope2 = create_test_envelope();

        let encoded1 = encode(&envelope1).expect("encode failed");
        let encoded2 = encode(&envelope2).expect("encode failed");

        let mut decoder = FrameDecoder::new();
        decoder.extend(&encoded1);
        decoder.extend(&encoded2);

        // Should decode two messages
        assert!(decoder.decode_next().expect("decode error").is_some());
        assert!(decoder.decode_next().expect("decode error").is_some());
        assert!(decoder.decode_next().expect("decode error").is_none());
    }

    #[test]
    fn test_message_too_large() {
        let mut buf = BytesMut::new();
        buf.put_u32(MAX_MESSAGE_SIZE + 1); // Length prefix exceeds max
        buf.put_bytes(0, 100); // Some dummy data

        let result = decode(&mut buf);
        assert!(matches!(result, Err(CodecError::InvalidLength(_))));
    }
}
