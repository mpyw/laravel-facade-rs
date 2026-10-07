#![doc = include_str!("../README.md")]

mod application;
mod error;
mod facade;
mod sync;

pub use application::{Application, ApplicationBuilder};
pub use error::Error;
pub use facade::{ExtendsFacade, Facade, Fake};
pub use laravel_facade_macros::extends_facade;

#[doc(hidden)]
pub use facade::__private;

/// Re-export so that users do not need a direct dependency.
pub use shaku;
