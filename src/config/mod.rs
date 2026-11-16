pub mod load;
pub mod schema;
pub mod theme;
pub mod validate;
pub mod watch;

pub use load::{default_config_commented, load_from_file_or_default, load_from_str, save_to_file};
pub use schema::Config;
pub use theme::{RgbColor, Theme};
pub use validate::{ConfigValidationError, KNOWN_COMMANDS};
pub use watch::ConfigWatcher;
