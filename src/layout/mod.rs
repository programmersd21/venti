pub mod floating;
pub mod focus;
pub mod resolve;
pub mod tree;
pub mod workspace;

pub use floating::{FloatingPane, clamp_rect};
pub use focus::{Direction, FocusManager};
pub use resolve::{choose_aspect_split, resolve};
pub use tree::{LayoutNode, PaneId, SplitAxis, SplitDirection};
pub use workspace::WorkspaceId;
