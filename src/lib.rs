//! Write plugins for [SimpleEdit](https://simple-edit-app.github.io/simpleedit/).
//!
//! A plugin is a program with a [`plugin.json`](manifest) beside it. The
//! editor starts it and they talk over its standard input and output
//! ([`protocol`]); what its panels show is a tree of [`ui`] nodes that the
//! editor draws. With the `runtime` feature (on by default), [`run`] does
//! the talking: implement [`Plugin`] and react.
//!
//! The guide: <https://simple-edit-app.github.io/simpleedit/plugins.html>.

pub mod lint;
pub mod manifest;
pub mod protocol;
pub mod ui;

#[cfg(feature = "runtime")]
mod runtime;

#[cfg(feature = "runtime")]
pub use runtime::{run, run_on, Host, Plugin};
