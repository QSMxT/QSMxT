use crate::bids::discovery::{self, DiscoveryFilter};
use crate::cli::ValidateArgs;

pub fn execute(args: ValidateArgs) -> crate::Result<()> {
    let filter = DiscoveryFilter {
        include: args.include,
        exclude: args.exclude,
        ..Default::default()
    };

    let runs = discovery::discover_runs(&args.bids_dir, &filter)?;

    if runs.is_empty() {
        println!("No QSM-compatible runs found in {}", args.bids_dir.display());
        println!();
        println!("Expected BIDS structure:");
        println!("  sub-*/[ses-*/]anat/*_part-phase_*.nii[.gz]");
        println!("  with matching JSON sidecars containing EchoTime and MagneticFieldStrength");
        return Ok(());
    }

    let mut subjects: Vec<&str> = runs.iter().map(|r| r.key.subject.as_str()).collect();
    subjects.sort();
    subjects.dedup();

    println!("BIDS directory: {}", args.bids_dir.display());
    println!("Subjects: {}", subjects.len());
    println!("Total runs: {}", runs.len());
    println!();

    for run in &runs {
        println!("  {}", run.key);
        println!("    Echoes: {}", run.echoes.len());
        println!("    Echo times: {:?} s", run.echo_times);
        println!("    Field strength: {:.1} T", run.magnetic_field_strength);
        println!("    B0 direction: ({:.2}, {:.2}, {:.2})", run.b0_dir.0, run.b0_dir.1, run.b0_dir.2);

        let has_mag = run.echoes.iter().all(|e| e.magnitude_nifti.is_some());
        if has_mag {
            println!("    Magnitude: present");
        } else {
            println!("    Magnitude: MISSING (some algorithms may not work)");
        }
        println!();
    }

    Ok(())
}
