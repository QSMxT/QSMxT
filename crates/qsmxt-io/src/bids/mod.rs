pub mod bidsprov;
pub mod dataset_description;
pub mod derivatives;
pub mod discovery;
pub mod orientation;
pub mod sidecar;
/// The names `qsmxt run` writes, in qsmxt-engine so a front end names its files the same way.
pub use qsmxt_engine::bids::{entities, naming};
