//! What a run reports while it runs: which step it is on, how far an iterative stage has got,
//! what it has to say, and every output as it is produced.
//!
//! One observer serves the whole run. `qsmxt` draws progress bars and writes its log from it;
//! qsmbly posts the same events to the page. A stage that runs a qsm-core pipeline function hands
//! qsm-core its progress (or, where qsm-core takes an observer, a [`Core`] wrapping it), so
//! qsm-core reports through the same one.

/// Where a step of a run has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageEvent<'a> {
    /// The step is being computed; the text says what it is doing, for a status line.
    Started(&'a str),
    /// The step's earlier output is still valid, so it is not computed again.
    Cached,
    /// The step is computed and its outputs are kept; the text names it for the log.
    Finished(&'a str),
}

/// Receives a run's progress as it happens. Every method has a default that ignores the event.
pub trait Observer {
    /// Step `step` (its cache name, e.g. `mask` or `invert-reliable`), computed by `algorithm`
    /// when it has one, reached `event`.
    fn stage(&mut self, _step: &str, _algorithm: Option<&str>, _event: StageEvent<'_>) {}

    /// Iteration `done` of `total` of the stage being computed.
    fn progress(&mut self, _done: usize, _total: usize) {}

    /// A line for the run's log. (The stages' own detail goes through the `log` crate.)
    fn log(&mut self, _level: log::Level, _message: &str) {}

    /// A step produced `item`. The volume is only valid for the duration of the call.
    fn output(&mut self, _item: &crate::store::Item, _volume: qsm_core::pipeline::VolumeRef<'_>) {}
}

/// Ignores everything.
impl Observer for () {}

/// An engine observer as qsm-core's: qsm-core's progress reaches it unchanged. Intermediate
/// outputs qsm-core reports are kept in [`Core::outputs`], in the order they came, for the stage
/// to return; storing them is the front end's, and can fail, which qsm-core's observer cannot.
pub struct Core<'a> {
    observer: &'a mut dyn Observer,
    /// The intermediate outputs qsm-core reported, by its name for each.
    pub outputs: Vec<(&'static str, VolumeData)>,
}

impl<'a> Core<'a> {
    pub fn new(observer: &'a mut dyn Observer) -> Self {
        Self { observer, outputs: Vec::new() }
    }
}

impl qsm_core::pipeline::Observer for Core<'_> {
    fn progress(&mut self, done: usize, total: usize) {
        self.observer.progress(done, total)
    }

    fn output(&mut self, name: &'static str, volume: qsm_core::pipeline::VolumeRef<'_>) {
        self.outputs.push((name, VolumeData::from(volume)));
    }
}

/// A volume a stage produced, owned.
#[derive(Debug, Clone, PartialEq)]
pub enum VolumeData {
    /// A real-valued volume, in the units its stage documents.
    Real(Vec<f64>),
    /// A binary mask (1 inside, 0 outside).
    Mask(Vec<u8>),
}

impl VolumeData {
    pub fn as_ref(&self) -> qsm_core::pipeline::VolumeRef<'_> {
        match self {
            VolumeData::Real(v) => qsm_core::pipeline::VolumeRef::Real(v),
            VolumeData::Mask(m) => qsm_core::pipeline::VolumeRef::Mask(m),
        }
    }
}

impl From<qsm_core::pipeline::VolumeRef<'_>> for VolumeData {
    fn from(v: qsm_core::pipeline::VolumeRef<'_>) -> Self {
        match v {
            qsm_core::pipeline::VolumeRef::Real(v) => VolumeData::Real(v.to_vec()),
            qsm_core::pipeline::VolumeRef::Mask(m) => VolumeData::Mask(m.to_vec()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Recorder(Vec<(usize, usize)>);
    impl Observer for Recorder {
        fn progress(&mut self, done: usize, total: usize) {
            self.0.push((done, total));
        }
    }

    /// qsm-core's progress reaches the engine observer, and its outputs are kept in order.
    #[test]
    fn core_forwards_progress_and_keeps_outputs() {
        use qsm_core::pipeline::{Observer as _, VolumeRef};
        let mut rec = Recorder::default();
        let mut core = Core::new(&mut rec);
        qsm_core::pipeline::Observer::progress(&mut core, 1, 3);
        qsm_core::pipeline::Observer::progress(&mut core, 2, 3);
        core.output("a", VolumeRef::Real(&[1.0, 2.0]));
        core.output("b", VolumeRef::Mask(&[1, 0]));
        let outputs = std::mem::take(&mut core.outputs);
        assert_eq!(outputs, vec![("a", VolumeData::Real(vec![1.0, 2.0])), ("b", VolumeData::Mask(vec![1, 0]))]);
        assert_eq!(rec.0, vec![(1, 3), (2, 3)]);
    }
}
