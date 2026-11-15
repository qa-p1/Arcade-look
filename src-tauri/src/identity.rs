//! Who Look is to the desktop, Arcade Link and the user, in one place, so the launcher, the window, the login item,
//! notifications and the Link manifest can't drift apart.

/// The Arcade Link id.
pub const ME: &str = arcade_link::ids::LOOK;
pub const NAME: &str = "Arcade Look";
/// The terminal command, and the name of Look's cache folder and log.
pub const CLI_NAME: &str = "arcade-look";
/// The OS identifier: Tauri's app identifier (tauri.conf.json), the desktop entry, GTK's program name and WM_CLASS.
pub const DESKTOP_ID: &str = "dev.arcade.Look";
