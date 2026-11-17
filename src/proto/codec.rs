use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use thiserror::Error;

/// current wire protocol version.
pub const PROTOCOL_VERSION: u16 = 2;

/// maximum payload frame size allowed (32 mib). scrolled/fullscreen images
/// can legitimately exceed 1 mib (a 60x30 kitty image base64 encodes to ~5 mib),
/// so frames must be sized for realistic full-screen image payloads.
pub const MAX_FRAME_SIZE: usize = 32 * 1024 * 1024;

/// protocol codec errors.
#[derive(Debug, Error)]
pub enum CodecError {
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("postcard serialization error: {0}")]
    Serialize(#[from] postcard::Error),
    #[error("postcard deserialization error: {0}")]
    Deserialize(postcard::Error),
    #[error("frame size {0} exceeds maximum limit {1}")]
    FrameTooLarge(usize, usize),
}

/// buffered decoder for streams containing fragmented or back-to-back frames.
pub struct FrameDecoder<T> {
    buf: Vec<u8>,
    offset: usize,
    message: PhantomData<fn() -> T>,
}

impl<T> Default for FrameDecoder<T> {
    fn default() -> Self {
        Self {
            buf: Vec::with_capacity(65536),
            offset: 0,
            message: PhantomData,
        }
    }
}

impl<T: DeserializeOwned> FrameDecoder<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// appends stream bytes, compacting consumed bytes at most once per append.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.offset > 0 {
            self.buf.copy_within(self.offset.., 0);
            self.buf.truncate(self.buf.len() - self.offset);
            self.offset = 0;
        }
        self.buf.extend_from_slice(bytes);
    }

    /// decodes the next complete frame, retaining any incomplete suffix.
    pub fn decode_next(&mut self) -> Result<Option<T>, CodecError> {
        let Some((message, consumed)) = decode_from_slice(&self.buf[self.offset..])? else {
            return Ok(None);
        };
        self.offset += consumed;
        if self.offset == self.buf.len() {
            self.buf.clear();
            self.offset = 0;
        }
        Ok(Some(message))
    }
}

/// encodes a serializable message into a 4-byte little endian length prefixed postcard buffer.
pub fn encode<T: Serialize>(msg: &T) -> Result<Vec<u8>, CodecError> {
    let payload = postcard::to_allocvec(msg)?;
    if payload.len() > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(payload.len(), MAX_FRAME_SIZE));
    }
    let len = payload.len() as u32;
    let mut out = Vec::with_capacity(4 + payload.len());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// writes an encoded message directly to a writer.
pub fn write_msg<W: Write, T: Serialize>(w: &mut W, msg: &T) -> Result<(), CodecError> {
    let bytes = encode(msg)?;
    w.write_all(&bytes)?;
    w.flush()?;
    Ok(())
}

/// decodes a length-prefixed postcard frame from raw bytes.
/// returns ok(some((msg, bytes_consumed))) when a complete frame is parsed.
pub fn decode_from_slice<T: DeserializeOwned>(
    buf: &[u8],
) -> Result<Option<(T, usize)>, CodecError> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(len, MAX_FRAME_SIZE));
    }
    let total = 4 + len;
    if buf.len() < total {
        return Ok(None);
    }

    let payload = &buf[4..total];
    let msg = postcard::from_bytes(payload).map_err(CodecError::Deserialize)?;
    Ok(Some((msg, total)))
}

/// reads a single complete message from a reader.
pub fn read_msg<R: Read, T: DeserializeOwned>(r: &mut R) -> Result<T, CodecError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(len, MAX_FRAME_SIZE));
    }

    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    let msg = postcard::from_bytes(&payload).map_err(CodecError::Deserialize)?;
    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::FrameDecoder;
    use crate::proto::{ClientMsg, encode};

    #[test]
    fn decoder_handles_fragmented_and_back_to_back_frames_without_per_frame_compaction() {
        let frame = encode(&ClientMsg::Detach).expect("encode detach");
        let mut stream = Vec::with_capacity(frame.len() * 10_000);
        for _ in 0..10_000 {
            stream.extend_from_slice(&frame);
        }

        let mut decoder = FrameDecoder::<ClientMsg>::new();
        decoder.push(&stream[..3]);
        assert!(decoder.decode_next().expect("partial header").is_none());
        decoder.push(&stream[3..]);

        let buffered_len = decoder.buf.len();
        assert_eq!(
            decoder.decode_next().expect("first frame"),
            Some(ClientMsg::Detach)
        );
        assert_eq!(decoder.offset, frame.len());
        assert_eq!(decoder.buf.len(), buffered_len);

        for _ in 1..10_000 {
            assert_eq!(
                decoder.decode_next().expect("complete frame"),
                Some(ClientMsg::Detach)
            );
        }
        assert_eq!(decoder.offset, 0);
        assert!(decoder.buf.is_empty());
        assert!(decoder.decode_next().expect("empty decoder").is_none());
    }
}
