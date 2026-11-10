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

    let plans = dicom::plan_session(&session);
    println!("\nDetected {} unique series (auto-classified):", unique.len());
    for line in classification_lines(&session, &unique, &plans) {
        println!("{line}");
    }
    for line in report_lines(&session.report) {
        println!("{line}");
    }

    if args.dry_run {
        println!("\n(dry run — not converting)");
        return Ok(());
    }

    println!("\nConverting to BIDS at {}...", args.output_dir.display());
    let (tx, rx) = mpsc::channel();
    let out = args.output_dir.clone();
    let options = dicom::convert::ConvertOptions { keep_dicom_uids: args.keep_dicom_uids };
    let handle = std::thread::spawn(move || {
        dicom::convert::convert_session_streaming(&session, &out, options, &tx);
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

/// One line per unique series: acquisition, description, type, echoes, coils, subjects, and the
/// name it gets.
pub fn classification_lines(session: &dicom::DicomSession, unique: &[dicom::UniqueSeries], plans: &[dicom::SeriesPlan]) -> Vec<String> {
    unique
        .iter()
        .map(|g| {
            let s = session.series_ref(&g.refs[0]);
            let echo = match g.echo_times.as_slice() {
                [] => String::new(),
                [te] => format!(" TE={:.1}ms", te),
                tes => format!(" {}×TEs=[{:.1}…{:.1}]ms", tes.len(), tes[0], tes[tes.len() - 1]),
            };
            let n_subjects = g.subject_count();
            let subjects = if n_subjects > 1 { format!(" ×{} subjects", n_subjects) } else { String::new() };
            let coil = if s.coil_type == dicom::CoilType::Uncombined {
                format!(" (uncombined, {} coils)", s.coil_groups.len())
            } else {
                String::new()
            };
            let target = plans.iter().find(|p| p.r == g.refs[0]).map(|p| unique_target_label(g, &p.target)).unwrap_or_default();
            let note = s.note.as_ref().map(|n| format!(" ({n})")).unwrap_or_default();
            format!(
                "  acq-{:<22} {:<26} [{}]{}{}{} -> {}{}",
                g.acq_name, s.description, s.series_type.label(), echo, coil, subjects, target, note
            )
        })
        .collect()
}

/// Where a unique series goes: like [`target_label`], with `echo-<N>` when its instances (one
/// series per echo) hold several echoes.
pub fn unique_target_label(g: &dicom::UniqueSeries, t: &dicom::Target) -> String {
    let label = target_label(t);
    if g.echo_times.len() > 1 && g.refs.len() > 1 {
        label.replacen("_echo-1_", "_echo-<N>_", 1)
    } else {
        label
    }
}

/// Where a series goes, in a few words.
pub fn target_label(t: &dicom::Target) -> String {
    match t {
        dicom::Target::Bids(stem) => stem.preview(),
        dicom::Target::Extra { prefix, .. } => format!("extra_data/{prefix}"),
        dicom::Target::Skip => "not converted".to_string(),
    }
}

/// The scan's warnings, notes and skipped series.
pub fn report_lines(r: &dicom::ScanReport) -> Vec<String> {
    let mut out = Vec::new();
    if r.not_dicom > 0 {
        out.push(format!("\n{} file{} not DICOM and {} ignored.", r.not_dicom, if r.not_dicom == 1 { " is" } else { "s are" }, if r.not_dicom == 1 { "was" } else { "were" }));
    }
    if !r.skipped.is_empty() {
        out.push("\nNot images, not converted:".to_string());
        out.extend(r.skipped.iter().map(|s| format!("  {s}")));
    }
    if !r.notes.is_empty() {
        out.push("\nNotes:".to_string());
        out.extend(r.notes.iter().map(|s| format!("  {s}")));
    }
    if !r.warnings.is_empty() {
        out.push("\nWarnings:".to_string());
        out.extend(r.warnings.iter().map(|s| format!("  {s}")));
    }
    out
}
