use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// Whether this build includes deep-learning (ONNX) support — shown in `--version` so a
/// non-`dl` build (e.g. the Windows/ARM64 binary) advertises the missing models up front.
#[cfg(feature = "dl")]
const DL_STATUS: &str = "deep-learning models: enabled";
#[cfg(not(feature = "dl"))]
const DL_STATUS: &str = "deep-learning models: DISABLED in this build (classical algorithms only)";

/// The maps `--dicom-outputs` can name: the exported series' suffixes, in lower case (matched
/// case-insensitively). One value per use, so a list cannot swallow the BIDS directory after it.
pub const DICOM_OUTPUT_TOKENS: [&str; 17] = [
    "magnitude", "chimap", "swi", "minip", "desc-swi_phase", "desc-paramagnetic_smwi", "desc-diamagnetic_smwi", "desc-paramagneticsmwi_minip",
    "desc-diamagneticsmwi_minip", "t2starmap", "r2starmap", "r2map", "r2primemap",
    "desc-paramagnetic_chimap", "desc-diamagnetic_chimap", "desc-total_chimap", "dseg",
];

#[derive(Parser, Debug)]
#[command(
    name = "qsmxt",
    version,
    long_version = const_format::formatcp!(
        "{}{}\nQSM.rs (qsm-core): {}\n{}",
        env!("CARGO_PKG_VERSION"),
        crate::commands::update::BUILD_COMMIT_SUFFIX,
        env!("QSM_CORE_DESCRIBE"),
        DL_STATUS,
    ),
    about = "QSMxT: Quantitative Susceptibility Mapping tool (Rust)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Run the full QSM pipeline on a BIDS dataset
    Run(RunArgs),
    /// Generate a pipeline configuration file
    Init(InitArgs),
    /// Validate BIDS dataset structure for QSM processing
    Validate(ValidateArgs),
    /// Convert a DICOM directory to BIDS using automatic series classification
    #[command(name = "dicom-convert")]
    DicomConvert(DicomConvertArgs),
    /// Generate SLURM job scripts for HPC execution: one job per run, then an aggregation job
    Slurm(SlurmArgs),
    /// Run the dataset-level steps over runs already processed with `qsmxt run
    /// --no-dataset-steps` (multi-orientation, summary tables, provenance, clean-up)
    Aggregate(AggregateArgs),
    /// Masking operations
    Mask {
        #[command(subcommand)]
        command: MaskCommand,
    },
    /// Multi-coil phase combination (uncombined receive-coil channels -> combined echoes)
    Combine {
        #[command(subcommand)]
        command: CombineCommand,
    },
    /// Phase unwrapping
    Unwrap {
        #[command(subcommand)]
        command: UnwrapCommand,
    },
    /// B0 field mapping from multi-echo phase (NIfTI in, ppm out)
    Fieldmap {
        #[command(subcommand)]
        command: FieldmapCommand,
    },
    /// Background field removal
    Bgremove {
        #[command(subcommand)]
        command: BgremoveCommand,
    },
    /// Dipole inversion
    Invert {
        #[command(subcommand)]
        command: InvertCommand,
    },
    /// Susceptibility source separation
    Separate {
        #[command(subcommand)]
        command: SeparateCommand,
    },
    /// QSMART vessel-aware reconstruction: total field -> susceptibility
    Qsmart(QsmartArgs),
    /// Susceptibility-weighted imaging
    Swi(SwiArgs),
    /// Whole-brain segmentation of a magnitude image with SynthSeg
    Segment(SegmentArgs),
    /// R2* mapping from multi-echo magnitude data
    R2star(R2starArgs),
    /// T2* mapping from multi-echo magnitude data
    T2star(T2starArgs),
    /// R2 mapping from multi-echo spin-echo (MESE) magnitude data (EPG)
    R2(R2Args),
    /// R2' mapping: measured as R2* - R2, or estimated from R2* by R2PRIMEnet
    R2prime(R2primeArgs),
    /// Inhomogeneity correction on magnitude data (makehomogeneous or N4)
    Homogeneity(HomogeneityArgs),
    /// Resample oblique volume to axial orientation
    Resample(ResampleArgs),
    /// Compute ROMEO phase quality map
    #[command(name = "quality-map")]
    QualityMap(QualityMapArgs),
    /// Download an example BIDS dataset to try the pipeline on
    Example(ExampleArgs),
    /// Launch interactive TUI for pipeline configuration
    Tui,
    /// Check for updates and optionally install the latest version
    Update(UpdateArgs),
    /// Show the announcements from the QSMxT team, newest first
    News,
}

#[derive(Parser, Debug)]
pub struct ExampleArgs {
    /// Directory to create or add the BIDS dataset in [default: qsmxt-example]
    pub output_dir: Option<PathBuf>,

    /// Which acquisition to fetch; repeat to add several to the one dataset
    /// [default: prisma-bridge-run1]
    #[arg(short, long, value_delimiter = ',')]
    pub name: Vec<String>,

    /// List the available acquisitions and exit
    #[arg(long)]
    pub list: bool,

    /// Rewrite acquisitions that are already present in the dataset
    #[arg(long)]
    pub force: bool,
}

#[derive(Parser, Debug)]
pub struct UpdateArgs {
    /// Update without prompting for confirmation
    #[arg(short, long)]
    pub yes: bool,

    /// Replace a development build (version 0.0.0) with the latest release. Without
    /// it, `qsmxt update` refuses on a dev build, since every release would look newer
    #[arg(long, conflicts_with = "main")]
    pub force: bool,

    /// Build the latest commit on the main branch from source and install it in place of
    /// this qsmxt. Needs the Rust toolchain (https://rustup.rs). The build is kept in
    /// ~/.qsmxt/build, so the next update rebuilds only what changed. `qsmxt update --force`
    /// goes back to the latest release
    #[arg(long)]
    pub main: bool,
}

#[derive(Parser, Debug)]
pub struct DicomConvertArgs {
    /// Input DICOM directory (searched recursively)
    pub dicom_dir: PathBuf,

    /// Output BIDS directory
    pub output_dir: PathBuf,

    /// Print the auto-detected series classification and exit without converting
    #[arg(long)]
    pub dry_run: bool,

    /// Keep the study's DICOM UIDs (Study, Series and Frame of Reference) and dates in the
    /// sidecars, so `qsmxt run --export-dicom` files its series into the original study. Patient
    /// identity is still removed, but the UIDs link the dataset to the original study: don't
    /// share such a dataset outside your site
    #[arg(long)]
    pub keep_dicom_uids: bool,
}


mod bgremove;
mod enums;
mod fieldmap;
mod invert;
mod mask;
mod multi_orient;
mod params;
mod parsers;
mod pipeline;
mod separate;
mod standalone;
mod unwrap;

pub use bgremove::*;
pub use enums::*;
pub use fieldmap::*;
pub use invert::*;
pub use mask::*;
pub use multi_orient::*;
pub use params::*;
pub use parsers::*;
pub use pipeline::*;
pub use separate::*;
pub use standalone::*;
pub use unwrap::*;

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from([&["qsmxt"][..], args].concat())
            .unwrap_or_else(|e| panic!("{args:?} did not parse:\n{e}"))
    }

    #[test]
    fn combine_mcpc3ds_takes_prelude_settings_for_the_hip_unwrap() {
        let cli = parse(&["combine", "mcpc3ds", "--phase", "p.nii", "--magnitude", "m.nii", "-o", "o",
            "--hip-unwrapping", "prelude", "--prelude-labelling", "volume", "--prelude-partitions", "6"]);
        let Command::Combine { command: CombineCommand::Mcpc3ds(a) } = cli.command
            else { panic!("expected combine mcpc3ds") };
        assert!(matches!(a.hip_unwrapping, UnwrapAlgorithmArg::Prelude));
        assert_eq!(a.prelude_params.to_prelude_params(), qsm_core::unwrap::PreludeParams {
            num_phase_partitions: 6,
            labelling: qsm_core::unwrap::PreludeLabelling::Volume,
            ..Default::default()
        });
    }

    /// The defaults the V-SHARP radius step and χ-sep iLSQR's QSM help states are qsm-core's.
    #[test]
    fn stated_reference_defaults_are_qsm_cores() {
        use clap::CommandFactory;
        let cli = Cli::command();
        let help = |path: &[&str], flag: &str| {
            let cmd = path.iter().fold(&cli, |c, name| c.find_subcommand(name).unwrap());
            let arg = cmd.get_arguments().find(|a| a.get_long() == Some(flag)).unwrap_or_else(|| panic!("no --{flag}"));
            arg.get_help().unwrap().to_string()
        };
        let v = qsm_core::bgremove::VsharpParams::default();
        let q = qsm_core::separation::ChiSepIlsqrParams::default().qsm_ilsqr;
        for (path, flag, default) in [
            (&["run"][..], "vsharp-radius-step", v.radius_step.to_string()),
            (&["bgremove", "vsharp"][..], "radius-step", v.radius_step.to_string()),
            (&["run"][..], "chi-sep-ilsqr-qsm-max-iter", q.max_iter.to_string()),
            (&["run"][..], "chi-sep-ilsqr-qsm-pad-mm", q.pad_mm.to_string()),
        ] {
            let h = help(path, flag);
            assert!(h.contains(&format!("(default: {default})")) || h.contains(&format!("(default: {default};")), "--{flag}: {h}");
        }
    }

    /// The standalone subcommands take the boundary rim, NDI's weight normaliser and WH-QSM's
    /// flat-in-mask weight by the run flags' names.
    #[test]
    fn standalone_rim_and_weight_options() {
        for (alg, name) in [("harperella", "sti"), ("iharperella", "whole")] {
            let cli = parse(&["bgremove", alg, "f.nii", "-m", "m.nii", "-o", "o.nii", "--rim", name]);
            let got = match cli.command {
                Command::Bgremove { command: BgremoveCommand::Harperella(a) } => a.settings.values("harperella-rim").map(|v| v.join(" ")),
                Command::Bgremove { command: BgremoveCommand::Iharperella(a) } => a.settings.values("iharperella-rim").map(|v| v.join(" ")),
                _ => panic!("expected bgremove {alg}"),
            };
            assert_eq!(got.as_deref(), Some(name));
        }
        let cli = parse(&["invert", "ndi", "f.nii", "-m", "m.nii", "-o", "o.nii", "--magnitude", "mag.nii",
            "--field-strength", "3", "--echo-times", "0.02", "--weight-norm", "mask-p999"]);
        let Command::Invert { command: InvertCommand::Ndi(a) } = cli.command else { panic!("expected invert ndi") };
        assert_eq!(a.settings.values("ndi-weight-norm"), Some(vec!["mask-p999"]));
        let cli = parse(&["invert", "whqsm", "f.nii", "-m", "m.nii", "-o", "o.nii", "--field-strength", "3", "--echo-times", "0.02",
            "--weight", "flat-in-mask"]);
        let Command::Invert { command: InvertCommand::Whqsm(a) } = cli.command else { panic!("expected invert whqsm") };
        assert_eq!(a.settings.values("whqsm-weight"), Some(vec!["flat-in-mask"]));
        assert!(Cli::try_parse_from(["qsmxt", "bgremove", "harperella", "f.nii", "-m", "m.nii", "-o", "o.nii",
            "--rim", "edge"]).is_err());
    }

    /// Oblique slabs give B0 directions with negative components, written as `-0.05`. Clap
    /// reads a leading `-` as a flag unless the argument allows negative numbers, so every
    /// `--b0-direction` must, in every position of the triple.
    const DIRECTIONS: [[&str; 3]; 3] = [
        ["-0.05", "0.09", "0.99"],
        ["0.05", "-0.09", "0.99"],
        ["0.05", "0.09", "-0.99"],
    ];

    fn expected(d: &[&str; 3]) -> Vec<f64> {
        d.iter().map(|v| v.parse().unwrap()).collect()
    }

    #[test]
    fn bgremove_vsharp_accepts_negative_b0_direction() {
        for d in DIRECTIONS {
            let cli = parse(&[&["bgremove", "vsharp", "f.nii", "-m", "m.nii", "-o", "o.nii",
                "--b0-direction"][..], &d].concat());
            let Command::Bgremove { command: BgremoveCommand::Vsharp(a) } = cli.command
                else { panic!("expected bgremove vsharp") };
            assert_eq!(a.common.b0_direction, expected(&d));
        }
    }

    #[test]
    fn invert_accepts_negative_b0_direction() {
        for d in DIRECTIONS {
            let cli = parse(&[&["invert", "tkd", "f.nii", "-m", "m.nii", "-o", "o.nii",
                "--b0-direction"][..], &d].concat());
            let Command::Invert { command: InvertCommand::Tkd(a) } = cli.command
                else { panic!("expected invert tkd") };
            assert_eq!(a.common.b0_direction, expected(&d));
        }
        // A negative B0 component followed by another flag still ends the triple there.
        let cli = parse(&["invert", "rts", "f.nii", "-m", "m.nii", "-o", "o.nii",
            "--b0-direction", "-0.05", "-0.09", "0.99", "--tol", "1e-3"]);
        let Command::Invert { command: InvertCommand::Rts(a) } = cli.command
            else { panic!("expected invert rts") };
        assert_eq!(a.common.b0_direction, vec![-0.05, -0.09, 0.99]);
        assert_eq!(a.settings.values("rts-tol"), Some(vec!["1e-3"]));
    }

    #[test]
    fn cosmos_accepts_repeated_negative_b0_directions() {
        let cli = parse(&["invert", "cosmos", "-i", "a.nii", "-i", "b.nii", "-m", "m.nii",
            "-o", "o.nii", "--b0-direction", "-0.05", "0.09", "0.99",
            "--b0-direction", "0.05", "-0.09", "-0.99"]);
        let Command::Invert { command: InvertCommand::Cosmos(a) } = cli.command
            else { panic!("expected invert cosmos") };
        assert_eq!(a.common.b0_direction, vec![-0.05, 0.09, 0.99, 0.05, -0.09, -0.99]);
    }

    #[test]
    fn separate_and_qsmart_accept_negative_b0_direction() {
        let cli = parse(&["separate", "r2star-qsm", "--qsm", "q.nii", "-m", "m.nii", "-o", "out",
            "--r2star", "r.nii", "--b0-direction", "0.05", "-0.09", "0.99"]);
        let Command::Separate { command: SeparateCommand::R2starQsm(a) } = cli.command
            else { panic!("expected separate r2star-qsm") };
        assert_eq!(a.common.b0_direction, vec![0.05, -0.09, 0.99]);

        let cli = parse(&["qsmart", "f.nii", "-m", "m.nii", "-o", "o.nii", "--field-strength", "3",
            "--echo-time", "0.02", "--b0-direction", "0.05", "0.09", "-0.99"]);
        let Command::Qsmart(a) = cli.command else { panic!("expected qsmart") };
        assert_eq!(a.b0_direction, vec![0.05, 0.09, -0.99]);
    }

    /// A negative threshold parses (so the run can say what to use instead) and is refused when
    /// applied; `--axial-resampling` is what turns resampling on.
    #[test]
    fn a_negative_obliquity_threshold_points_to_axial_resampling() {
        let cli = parse(&["run", "bids", "--obliquity-threshold", "-1"]);
        let Command::Run(run) = cli.command else { panic!("expected run") };
        let mut c = qsmxt_engine::PipelineConfig::default();
        let err = crate::cli_settings::apply_normalised(&mut c, &run.pipeline).unwrap_err().to_string();
        assert!(err.contains("--axial-resampling"), "{err}");
        let cli = parse(&["run", "bids", "--axial-resampling", "--obliquity-threshold", "5"]);
        let Command::Run(run) = cli.command else { panic!("expected run") };
        crate::cli_settings::apply_normalised(&mut c, &run.pipeline).unwrap();
        assert!(c.pipeline.axial_resampling);
        assert_eq!(c.pipeline.obliquity_threshold, 5.0);
    }
}
