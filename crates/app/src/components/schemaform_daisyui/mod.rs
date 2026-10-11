//! daisyUI presentation for `schemaform-dioxus`: a control renderer, a structure
//! bundle (the shell), a finding presenter, and the seam its add-ons extend.
//!
//! See `README.md` beside this file for the component's scope, layout, and the
//! mapping it performs. The component's tests live in the crate's `tests/`
//! directory, so this directory ships no test code.

mod addon;
mod appearance;
mod boolean;
mod buttons;
mod choice;
mod component;
mod constant;
mod density;
mod findings;
mod mapping;
mod multiple_choice;
mod options;
mod parts;
mod shell;
mod text;

pub use addon::{Addon, Addons};
pub use appearance::Appearance;
pub use buttons::ButtonClasses;
pub use component::*;
pub use density::Density;
pub use findings::DaisyuiFindings;
pub use mapping::*;
pub use options::Options;
pub use shell::DaisyuiShell;

// The parts the add-on Components build on. Every installed Component lives in the host's crate,
// so these reach the add-ons without joining this package's public surface.
pub(crate) use buttons::ButtonRole;
pub(crate) use choice::placeholder;
pub(crate) use mapping::{choice_handles, use_focus_exit, use_no_commit};
pub(crate) use parts::{
    WidgetLayout, editable, editable_field, icon, kind_name, presence_affordances,
};
