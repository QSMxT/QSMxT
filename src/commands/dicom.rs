use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Arc};

use crate::cli::DicomConvertArgs;
use crate::dicom::{self, convert::ConvertMessage};
use crate::error::QsmxtError;

/// Non-interactive DICOM → BIDS conversion. Scans the directory, auto-classifies
/// every series (best guess — same logic the TUI uses), prints the result, and
/// converts. Use `--dry-run` to preview the classification without converting.
pub fn execute(args: DicomConvertArgs) -> crate::Result<()> {
    println!("Scanning {} for DICOM series...", args.dicom_dir.display());
    let session = dicom::scan_dicom_directory(&args.dicom_dir, Arc::new(AtomicUsize::new(0)))
        .map_err(QsmxtError::Dicom)?;

    let unique = session.unique_series();
    if unique.is_empty() {
        return Err(QsmxtError::Dicom(format!(
            "no DICOM series found in {}",
            args.dicom_dir.display()
        )));
    }

    println!("\nDetected {} unique series (auto-classified):", unique.len());
    for g in &unique {
        let s = session.series_ref(&g.refs[0]);
        let echo = match s.echo_times.as_slice() {
            [] => String::new(),
            [te] => format!(" TE={:.1}ms", te),
            tes => format!(" {}×TEs=[{:.1}…{:.1}]ms", tes.len(), tes[0], tes[tes.len() - 1]),
        };
        let n_subjects = g.subject_count();
        let subjects = if n_subjects > 1 {
            format!(" ×{} subjects", n_subjects)
        } else {
            String::new()
        };
        let coil = if s.coil_type == dicom::CoilType::Uncombined {
            format!(" (uncombined, {} coils)", s.coil_groups.len())
        } else {
            String::new()
        };
        let recon = dicom::recon_desc(&s.image_type)
            .map(|d| format!(" ({})", d))
            .unwrap_or_default();
        println!(
            "  acq-{:<22} {:<26} [{}]{}{}{}{}",
            g.acq_name, s.description, s.series_type.label(), echo, coil, recon, subjects
        );
    }

    if args.dry_run {
        println!("\n(dry run — not converting)");
        return Ok(());
    }

    println!("\nConverting to BIDS at {}...", args.output_dir.display());
    let (tx, rx) = mpsc::channel();
    let out = args.output_dir.clone();
    let handle = std::thread::spawn(move || {
        dicom::convert::convert_session_streaming(&session, &out, &tx);
    });

    let mut had_error = false;
    for msg in rx {
        match msg {
            ConvertMessage::Log(line) => println!("{}", line),
            ConvertMessage::Error(e) => {
                eprintln!("ERROR: {}", e);
                had_error = true;
            }
            ConvertMessage::Done { bids_dir } => {
                println!("\nBIDS written to {}", bids_dir.display());
            }
        }
    }
    let _ = handle.join();

    if had_error {
        return Err(QsmxtError::Dicom(
            "one or more series failed to convert (see errors above)".to_string(),
        ));
    }
    Ok(())
}
