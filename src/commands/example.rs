//! `qsmxt example` — fetch an example BIDS dataset to try the pipeline on.

use std::path::PathBuf;

use indicatif::{ProgressBar, ProgressStyle};

use crate::cli::ExampleArgs;
use crate::error::QsmxtError;
use crate::example::{self, materialize::Outcome};

pub fn execute(args: ExampleArgs) -> crate::Result<()> {
    if args.list {
        print_list();
        return Ok(());
    }

    // Resolve every name before downloading anything, so a typo fails immediately
    // rather than after a 100 MB download.
    let selected: Vec<&example::Example> = if args.name.is_empty() {
        vec![example::default_example()]
    } else {
        args.name
            .iter()
            .map(|n| {
                example::find(n).ok_or_else(|| {
                    QsmxtError::Example(format!(
                        "unknown example '{n}'. Run `qsmxt example --list` to see the {} available.",
                        example::EXAMPLES.len()
                    ))
                })
            })
            .collect::<crate::Result<_>>()?
    };

    let bids_dir = args.output_dir.unwrap_or_else(|| PathBuf::from("qsmxt-example"));
    let total: u64 = selected.iter().map(|e| e.bytes).sum();
    log::info!(
        "Fetching {} acquisition{} ({:.0} MB) into {}",
        selected.len(),
        if selected.len() == 1 { "" } else { "s" },
        total as f64 / 1e6,
        bids_dir.display()
    );

    let mut written = 0usize;
    let mut skipped = 0usize;
    for example in &selected {
        let zip = download(example)?;
        match example::materialize::materialize(
            example,
            &zip,
            &bids_dir,
            args.force,
            &mut |step| log::info!("{step}"),
        )? {
            Outcome::Written(echoes) => {
                written += 1;
                log::info!(
                    "Added {} as sub-01/ses-{}/anat (acq-{}, run-{}, {echoes} echoes)",
                    example.id,
                    example.scanner,
                    example.acq,
                    example.run
                );
            }
            Outcome::Skipped => {
                skipped += 1;
                log::info!(
                    "{} is already in {} — skipping (use --force to rewrite it)",
                    example.id,
                    bids_dir.display()
                );
            }
        }
    }

    if skipped > 0 {
        log::info!("{written} added, {skipped} already present");
    }
    log::info!("Dataset ready. Run it with:");
    log::info!("    qsmxt run {} {}", bids_dir.display(), "qsmxt-output");
    Ok(())
}

/// Download one archive, drawing a progress bar when stderr is a terminal.
fn download(example: &example::Example) -> crate::Result<PathBuf> {
    let cached = example::download::cache_path(example);
    if cached.is_file() {
        log::info!("Verifying cached {}", cached.display());
    }

    let bar = ProgressBar::new(example.bytes);
    bar.set_style(
        ProgressStyle::with_template(
            "  {msg} [{bar:30}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> "),
    );
    bar.set_message(example.id.to_string());
    // Nothing is drawn until the first progress callback, so a cache hit stays quiet.
    bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());

    let mut started = false;
    let path = example::download::ensure(example, &mut |done, total| {
        if !started {
            started = true;
            bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
            if total > 0 {
                bar.set_length(total);
            }
        }
        bar.set_position(done);
    })?;

    if started {
        bar.finish_and_clear();
        log::info!("Downloaded {} ({:.0} MB)", example.id, example.bytes as f64 / 1e6);
    } else {
        log::info!("Using cached {}", path.display());
    }
    Ok(path)
}

fn print_list() {
    println!("Example datasets (one in-vivo subject, QSM harmonization, MGH bays 4/5)");
    println!("Source: {}\n", example::DATASET_URL);
    println!("  {:<28} DESCRIPTION", "NAME");
    for e in example::EXAMPLES {
        let marker = if e.id == example::DEFAULT_ID { "*" } else { " " };
        println!("{marker} {:<28} {}", e.id, e.describe());
    }
    println!("\n  * default when --name is not given");
    println!("\nAll acquisitions are the same subject, so several can share one dataset:");
    println!("    qsmxt example --name prisma-bridge-run1 --name cima-bridge-run1 ~/data/example");
    println!("\nArchives are cached in {}", example::download::cache_dir().display());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(name: Vec<&str>, list: bool) -> ExampleArgs {
        ExampleArgs {
            output_dir: Some(PathBuf::from("/nonexistent-should-not-be-reached")),
            name: name.into_iter().map(String::from).collect(),
            list,
            force: false,
        }
    }

    #[test]
    fn list_succeeds_without_touching_the_network_or_disk() {
        execute(args(vec![], true)).unwrap();
    }

    #[test]
    fn an_unknown_name_fails_before_anything_is_downloaded() {
        // Names are resolved up front precisely so a typo does not cost a 100 MB fetch.
        let err = execute(args(vec!["prisma-bridge-run9"], false)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("prisma-bridge-run9"), "{msg}");
        assert!(msg.contains("--list"), "the message should point at --list: {msg}");
    }

    #[test]
    fn one_bad_name_among_good_ones_still_fails_up_front() {
        let err = execute(args(vec!["prisma-bridge-run1", "nope"], false)).unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }


    #[test]
    fn a_fetch_failure_is_reported_rather_than_leaving_a_half_built_dataset() {
        // Point the fetch at a dead mirror: execute must surface the failure and leave
        // no dataset directory behind for the caller to mistake for a good one.
        let _guard = example::download::env_lock();
        let cache = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let dataset = out.path().join("bids");
        std::env::set_var("QSMXT_EXAMPLE_CACHE", cache.path());
        // Port 1 on loopback: nothing listens, so this fails without leaving the machine.
        std::env::set_var(example::BASE_URL_ENV, "http://127.0.0.1:1");

        let mut a = args(vec!["prisma-bridge-run1"], false);
        a.output_dir = Some(dataset.clone());
        let err = execute(a).unwrap_err();

        assert!(matches!(err, QsmxtError::Example(_)), "{err}");
        assert!(!dataset.exists(), "a failed fetch must not leave a dataset directory");

        std::env::remove_var(example::BASE_URL_ENV);
        std::env::remove_var("QSMXT_EXAMPLE_CACHE");
    }

    #[test]
    fn every_registry_id_resolves() {
        // `--list` prints these, so each must be usable as a `--name`.
        for e in example::EXAMPLES {
            assert!(example::find(e.id).is_some(), "{} does not resolve", e.id);
        }
    }
}
