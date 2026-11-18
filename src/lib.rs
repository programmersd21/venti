#![forbid(unsafe_code)]
//! # venti
//!
//! venti is a fast, quiet, linux-only terminal multiplexer written in rust on top of ratatui.
//!
//! ## architecture
//!
//! venti adopts a client-server architecture:
//! - **server daemon**: manages sessions, workspaces, dwindle tiling layouts, ptys, and vt100 terminal emulation state.
//! - **client**: thin interactive terminal frontend forwarding raw user input and applying differential screen updates.
//! - **wire protocol**: length-prefixed postcard binary payloads over local unix domain sockets.

#[cfg(not(target_os = "linux"))]
compile_error!("venti is linux only (for now)");

pub mod cli;
pub mod cli_help;
pub mod client;
pub mod config;
pub mod input;
pub mod layout;
pub mod pane;
pub mod proto;
pub mod render;
pub mod server;
pub mod sixel;
pub mod util;

pub use config::{Config, Theme};
pub use layout::{LayoutNode, PaneId, SplitAxis};
