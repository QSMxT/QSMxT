use std::path::Path;

use crate::bids::discovery::{self, DiscoveryFilter, QsmRun};
use crate::cli::ValidateArgs;

/// Tool (derivatives subdirectory) names that provide a matching file for this run.
fn deriv_tools_with(bids_dir: &Path, run: &QsmRun, suffix_glob: &str, skip_desc: bool) -> Vec<String> {
    let deriv = bids_dir.join("derivatives");
    let mut tools = Vec::new();
    let entries = match std::fs::read_dir(&deriv) {
        Ok(e) => e,
        Err(_) => return tools,
    };
    let sub = format!("sub-{}", run.key.subject);
    let ses = run.key.session.as_ref().map(|s| format!("ses-{}", s));
    for e in entries.flatten() {
        let td = e.path();
        if !td.is_dir() {
            continue;
        }
        let anat = match &ses {
            Some(s) => td.join(&sub).join(s).join("anat"),
            None => td.join(&sub).join("anat"),
        };
        let pattern = format!("{}/{}", anat.display(), suffix_glob);
        if let Ok(paths) = glob::glob(&pattern) {
            let hit = paths
                .filter_map(|r| r.ok())
                .any(|p| !skip_desc || !p.to_string_lossy().contains("desc-"));
            if hit {
                if let Some(name) = td.file_name().and_then(|n| n.to_str()) {
                    tools.push(name.to_string());
                }
            }
        }
    }
    tools.sort();
    tools.dedup();
    tools
}

fn yes_no(ok: bool, reason_if_no: &str) -> String {
    if ok {
        "yes".to_string()
    } else {
        format!("no  — {}", reason_if_no)
    }
}

fn fmt_list(tools: &[String]) -> String {
    if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(", ")
    }
}

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
        println!();
        println!("Optional, for source separation (chi-separation) and relaxometry:");
        println!("  sub-*/[ses-*/]anat/*_part-mag_*.nii[.gz]   (magnitude — R2*/T2*, SWI, GRE-only separation)");
        println!("  sub-*/[ses-*/]anat/*_echo-*_MESE.nii[.gz]  (multi-echo spin-echo — R2, hence R2' for separation)");
        println!("  derivatives/<tool>/.../anat/*_Chimap|_R2map|_R2primemap|_mask.nii  (bring-your-own inputs)");
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
        let n_echoes = run.echoes.len();
        let has_mag = run.has_magnitude;
        let mese = run.mese.as_ref();

        println!("  {}", run.key);
        println!("    Echoes: {}", n_echoes);
        println!("    Echo times: {:?} s", run.echo_times);
        println!("    Field strength: {:.1} T", run.magnetic_field_strength);
        println!("    B0 direction: ({:.2}, {:.2}, {:.2})", run.b0_dir.0, run.b0_dir.1, run.b0_dir.2);
        println!("    Magnitude: {}", if has_mag { "present" } else { "MISSING (some algorithms may not work)" });
        match mese {
            Some(m) => println!("    MESE (spin-echo): present ({} echoes, {:?} s)", m.echo_times.len(), m.echo_times),
            None => println!("    MESE (spin-echo): not found"),
        }

        // Bring-your-own inputs discoverable under derivatives/.
        let custom_masks = deriv_tools_with(&args.bids_dir, run, "*_mask.nii*", false);
        let custom_qsm = deriv_tools_with(&args.bids_dir, run, "*_Chimap.nii*", true);
        let custom_r2 = deriv_tools_with(&args.bids_dir, run, "*_R2map.nii*", false);
        let custom_r2prime = deriv_tools_with(&args.bids_dir, run, "*_R2primemap.nii*", false);

        // Capability derivations.
        let r2star_ok = n_echoes >= 3 && has_mag;

        println!("    Capabilities:");
        println!("      QSM reconstruction:  yes");
        println!("      R2*/T2* mapping:     {}", yes_no(r2star_ok, "needs ≥3 echoes + magnitude"));
        println!("      SWI:                 {}", yes_no(has_mag, "needs magnitude"));
        println!("      Susceptibility source separation:");
        println!("        r2star-qsm (GRE):  {}", yes_no(r2star_ok, "needs ≥3 echoes + magnitude (for R2*)"));
        println!("        decompose  (GRE):  {}", yes_no(has_mag, "needs multi-echo magnitude"));
        let r2prime_reason = if r2star_ok {
            "no MESE acquisition and no custom R2'/R2 map"
        } else {
            "needs R2' (≥3-echo magnitude for R2*, plus MESE or a custom R2'/R2 map)"
        };
        let r2prime_how = if !custom_r2prime.is_empty() {
            format!("yes (custom R2' map: {})", fmt_list(&custom_r2prime))
        } else if mese.is_some() {
            "yes (R2' computed from MESE)".to_string()
        } else if r2star_ok && !custom_r2.is_empty() {
            format!("yes (R2* + custom R2 map: {})", fmt_list(&custom_r2))
        } else {
            format!("no  — {}", r2prime_reason)
        };
        println!("        R2'-based (chi-sep-ilsqr/medi, wavesep, hc-chisep):");
        println!("          {}", r2prime_how);

        println!("    Bring-your-own inputs (derivatives/):");
        println!("      masks:   {}", fmt_list(&custom_masks));
        println!("      QSM:     {}", fmt_list(&custom_qsm));
        println!("      R2 map:  {}", fmt_list(&custom_r2));
        println!("      R2' map: {}", fmt_list(&custom_r2prime));
        println!();
    }

    Ok(())
}
