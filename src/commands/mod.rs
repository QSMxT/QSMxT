pub mod common;
pub mod aggregate;
pub mod bgremove;
pub mod combine;
pub mod dicom;
pub mod example;
pub mod fieldmap;
pub mod homogeneity;
pub mod init;
pub mod invert;
pub mod mask;
pub mod news;
pub mod qsmart;
pub mod recommend;
pub mod quality_map;
pub mod r2star;
pub mod r2;
pub mod r2prime;
pub mod separate;
pub mod resample;
pub mod run;
pub mod slurm;
pub mod segment;
pub mod swi;
pub mod t2star;
pub mod unwrap;
pub mod update;
pub mod validate;

#[cfg(test)]
mod integration_tests;
/// Tests for the multi-orientation commands (`invert cosmos`, `invert sti`).
///
/// That COSMOS and STI recover a phantom is QSM.rs's to test (`cosmos::tests::
/// recovers_chi_from_simulated_orientations`, `sti::tests::recovers_tensor_from_simulated_orientations`).
/// These check what the commands hand qsm-core: every orientation's field with its own B0
/// direction, in order, and the mask. A command that dropped an orientation, or paired a field
/// with another's direction, would not match qsm-core given the same inputs.
#[cfg(test)]
mod multiorient_tests;
