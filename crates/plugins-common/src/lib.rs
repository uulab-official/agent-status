mod base;
mod detect;

pub use base::BasePluginState;
pub use detect::{command_exists_on_path, file_exists, read_json_file_if_exists};
