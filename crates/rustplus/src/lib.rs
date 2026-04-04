#![allow(clippy::module_name_repetitions)]

pub mod error;
pub mod client;
pub mod camera;
pub(crate) mod ratelimit;

pub mod proto {
    #![allow(clippy::all, clippy::pedantic, clippy::nursery)]
    #![allow(non_snake_case)]
    #![allow(non_camel_case_types)]
    include!(concat!(env!("OUT_DIR"), "/rustplus.rs"));
}

pub use error::{Error, Result};
pub use client::RustPlusClient;
