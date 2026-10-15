//! What each stage of a run computes, without the files around it.
//!
//! Every function here takes the stage's inputs as in-memory volumes on the run's working grid,
//! with the run's [`RunMetadata`](crate::scan::RunMetadata) and configuration, and returns its
//! outputs (and anything it found worth recording). Reading inputs, writing outputs, caching,
//! BIDS names and provenance are the host's: `qsmxt run` loads each stage's inputs from the files
//! an earlier stage wrote and saves what comes back; qsmbly keeps the volumes in memory. Both run
//! the same computation, so they cannot drift apart.
//!
//! Each stage module also builds that stage's parameters, the JSON its step is cached under
//! ([`crate::cache::step_cache_key`]): what changes the result is decided next to the code that
//! computes it.

pub mod phase;
pub mod magnitude;
pub mod mask;
pub mod field;
pub mod crop;
pub mod background;
pub mod inversion;
