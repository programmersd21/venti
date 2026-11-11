pub mod codec;
pub mod messages;

pub use codec::{
    CodecError, FrameDecoder, MAX_FRAME_SIZE, PROTOCOL_VERSION, decode_from_slice, encode,
    read_msg, write_msg,
};
pub use messages::{
    CellDiff, ClientMsg, CursorShape, ExitReason, GfxCmd, GfxFormat, HelloMsg, MouseActionKind,
    MouseButtonKind, MouseEventMsg, ProtoColor, ScreenUpdate, ServerMsg,
};
