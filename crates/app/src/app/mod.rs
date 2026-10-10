//! The window around the screens: its bar and layout, the stores it renders
//! from, launching and quitting, the menu, and the palette's commands.

pub(crate) mod bar;
pub(crate) mod claims;
pub(crate) mod commands;
pub(crate) mod crash;
pub(crate) mod kept;
pub(crate) mod launch;
pub(crate) mod layout;
#[cfg(feature = "desktop")]
pub(crate) mod menu;
pub(crate) mod quit;
pub(crate) mod settings;
pub(crate) mod stores;
