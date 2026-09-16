//! The bundled example dataset: a single in-vivo subject, downloaded on demand.
//!
//! The data is one subject from the 2026-08-20 MGH bays 4/5 QSM harmonization
//! acquisition, scanned on two Siemens 3T scanners (`prisma` = MAGNETOM Prisma Fit,
//! `cima` = MAGNETOM Cima.X) under four protocols x 3 runs. QSM-CI packs each
//! acquisition into a canonical `inputs/` bundle (4D magnitude + 4D phase in radians,
//! echoes sorted ascending, plus a `params.json` of recovered acquisition parameters)
//! and publishes them on the public OSF project `gkemr`.
//!
//! [`download`] fetches and caches one of those zips; [`materialize`] turns it into a
//! BIDS `sub-01/ses-<scanner>/anat/` tree that `qsmxt run` reads directly. Several
//! acquisitions can be materialized into the same dataset — they are all the same
//! subject, so they differ only in the `ses-`/`acq-`/`run-` entities.

pub mod download;
pub mod materialize;

/// One downloadable acquisition.
///
/// `sha256`/`bytes` are the published OSF values (OSF reports both in its file API), so a
/// truncated or tampered download is caught before anything is unpacked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Example {
    /// Registry key, matching the QSM-CI dataset id (e.g. `prisma-bridge-run1`).
    pub id: &'static str,
    /// Scanner, used as the BIDS `ses-` entity.
    pub scanner: &'static str,
    /// Protocol, used as the BIDS `acq-` entity (BIDS labels are alphanumeric, so
    /// `pulseq-online` becomes `pulseqonline`).
    pub acq: &'static str,
    /// Repeat number, used as the BIDS `run-` entity.
    pub run: u32,
    /// OSF osfstorage file id within project [`OSF_PROJECT`].
    pub osf_id: &'static str,
    /// Exact size of the zip in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the zip.
    pub sha256: &'static str,
}

impl Example {
    /// Download URL. A public OSF project serves `osf.io/download/<id>/` without a token;
    /// the bare waterbutler URL only 302-redirects and then 400s for anonymous callers.
    pub fn url(&self) -> String {
        format!("https://osf.io/download/{}/", self.osf_id)
    }

    /// Human-readable scanner name.
    pub fn scanner_label(&self) -> &'static str {
        match self.scanner {
            "prisma" => "MAGNETOM Prisma Fit",
            "cima" => "MAGNETOM Cima.X",
            other => other,
        }
    }

    /// Human-readable protocol name.
    pub fn acq_label(&self) -> &'static str {
        match self.acq {
            "bridge" => "bridge (product GRE)",
            "local" => "local (site protocol)",
            "pulseqonline" => "Pulseq, online recon",
            "pulseqoffline" => "Pulseq, offline recon",
            other => other,
        }
    }

    /// One-line description for `--list` and the TUI picker.
    pub fn describe(&self) -> String {
        format!(
            "{} - {}, run {} ({:.0} MB)",
            self.scanner_label(),
            self.acq_label(),
            self.run,
            self.bytes as f64 / 1e6,
        )
    }
}

/// Landing page for the public OSF project (`gkemr`) hosting the packed acquisitions.
/// Cited in the generated `dataset_description.json` and sidecars.
pub const DATASET_URL: &str = "https://osf.io/gkemr/";

/// The example used when `--name` is not given: an ordinary product-GRE acquisition.
pub const DEFAULT_ID: &str = "prisma-bridge-run1";

/// Every published acquisition. Prisma first, then Cima.X; within a scanner, by protocol
/// and run. (`prisma-pulseq-offline-run2` was never published and so has no entry.)
pub const EXAMPLES: &[Example] = &[
    Example { id: "prisma-bridge-run1", scanner: "prisma", acq: "bridge", run: 1,
        osf_id: "6a8c4b1138a9a828b5ff21a9", bytes: 102636620, sha256: "0105106f7d232434e49c07252920350338daa760dff4996e74788efa078ee2d5" },
    Example { id: "prisma-bridge-run2", scanner: "prisma", acq: "bridge", run: 2,
        osf_id: "6a8c4b32ee75b6b40871e6ee", bytes: 102876631, sha256: "b360fdb9768667a92809239f49b0ddb5a3c0ac4912614165df4831280dbe5ef6" },
    Example { id: "prisma-bridge-run3", scanner: "prisma", acq: "bridge", run: 3,
        osf_id: "6a8c4b5af307ed392097e4cd", bytes: 103221267, sha256: "caf03b8638621713b3f1920b1faa103f712a4a1513a3ce5c5734186cf2ed6c53" },
    Example { id: "prisma-local-run1", scanner: "prisma", acq: "local", run: 1,
        osf_id: "6a8c4b7730a739d7deff201d", bytes: 96269010, sha256: "24c42e7c7001968dd344568a171dc8d45fd5b7e455acb505a3eafdc824a75a5f" },
    Example { id: "prisma-local-run2", scanner: "prisma", acq: "local", run: 2,
        osf_id: "6a8c4b9b30a739d7deff2029", bytes: 96184302, sha256: "8862f6ba12483fc5abccd90399f3c78fdf30b8525d7c848e3519bfa88ac8dfe1" },
    Example { id: "prisma-local-run3", scanner: "prisma", acq: "local", run: 3,
        osf_id: "6a8c4bd681b67f0163ff205c", bytes: 96358471, sha256: "6c55a58058b5443a29d4b55f325d4558ca63c43ef2730900dc2abf4dc7fb6308" },
    Example { id: "prisma-pulseq-offline-run1", scanner: "prisma", acq: "pulseqoffline", run: 1,
        osf_id: "6a8c4c1e7bc7b55803ff226f", bytes: 224205905, sha256: "87d2011605c60eaae9fa3e6dd0cf5b1dcec2235e9f8a12ee4935be950a9ac4db" },
    Example { id: "prisma-pulseq-offline-run3", scanner: "prisma", acq: "pulseqoffline", run: 3,
        osf_id: "6a8c4c5a6978ebf30371e4f8", bytes: 220958640, sha256: "5c7a7bffa641ae73a0a8489177ad1323c9e566a180b7a6669743f9d2823be4cb" },
    Example { id: "prisma-pulseq-online-run1", scanner: "prisma", acq: "pulseqonline", run: 1,
        osf_id: "6a8c4c876978ebf30371e522", bytes: 103363065, sha256: "fb2bc282df85f4dcfeb26c24975cd2f7365252e6f4b6bf9f9c084ad48ceb1afb" },
    Example { id: "prisma-pulseq-online-run2", scanner: "prisma", acq: "pulseqonline", run: 2,
        osf_id: "6a8c4cbab601fc4f61ff2024", bytes: 103833771, sha256: "f6851f02a5cca88df942d39b295379704e787344e3d4a71f469af6862e75bae5" },
    Example { id: "prisma-pulseq-online-run3", scanner: "prisma", acq: "pulseqonline", run: 3,
        osf_id: "6a8c4ce537eebdf57e71e4fe", bytes: 103912746, sha256: "eb4ab5e323efe402096e32cebedb3eb82ff0e3617338dcc7800474c331cf3e97" },
    Example { id: "cima-bridge-run1", scanner: "cima", acq: "bridge", run: 1,
        osf_id: "6a8c48ffee95d1176a71e4f7", bytes: 101998696, sha256: "7c5a13476017907e7b51ba9ffb7894e508091c06240e3fd64346dd61fe431d55" },
    Example { id: "cima-bridge-run2", scanner: "cima", acq: "bridge", run: 2,
        osf_id: "6a8c491fee75b6b40871e5f2", bytes: 101854104, sha256: "f717e97f727523dc4c1052c0dc278f066abae4e1b253d27643f2d63d477b6f0e" },
    Example { id: "cima-bridge-run3", scanner: "cima", acq: "bridge", run: 3,
        osf_id: "6a8c493e1ebbe17e3e71e5d9", bytes: 101790208, sha256: "43a2734ff8602b2f565d842940a5ab6d97af50a41f5f26864f74d67f2b834e1b" },
    Example { id: "cima-local-run1", scanner: "cima", acq: "local", run: 1,
        osf_id: "6a8c49631ebbe17e3e71e616", bytes: 116893140, sha256: "4101ef19f9f80000878e994368b8eec88e943ded8062ff301d1d109b26a530db" },
    Example { id: "cima-local-run2", scanner: "cima", acq: "local", run: 2,
        osf_id: "6a8c498c3cdf773dd997e539", bytes: 117023810, sha256: "8744b4f8f82de52c86a270d803959badf7322af3ee14404eae8e3a516128c018" },
    Example { id: "cima-local-run3", scanner: "cima", acq: "local", run: 3,
        osf_id: "6a8c49aecab829d61c71e621", bytes: 116128176, sha256: "23bf5a1156d692de092f7b9c3ce1d7d61efc4ac9a2983193eafdc6ba963f15f5" },
    Example { id: "cima-pulseq-offline-run1", scanner: "cima", acq: "pulseqoffline", run: 1,
        osf_id: "6a8c49f0e76360318a97e40e", bytes: 225377103, sha256: "f5eee8465c5f3e68292758467236d2333ef28e96960b3001a129676ba51d3ffb" },
    Example { id: "cima-pulseq-offline-run2", scanner: "cima", acq: "pulseqoffline", run: 2,
        osf_id: "6a8c4a2be76360318a97e438", bytes: 226451489, sha256: "a1f2da356ddb47315d99464b7f722e9c6af67f089e52663baf9888efa70ef775" },
    Example { id: "cima-pulseq-offline-run3", scanner: "cima", acq: "pulseqoffline", run: 3,
        osf_id: "6a8c4a679f28eb496897e570", bytes: 223541830, sha256: "6b9856c8d51e8da881d28b59c6ad571f0d80b1f6e81018e2aa10d57e54e4ac6e" },
    Example { id: "cima-pulseq-online-run1", scanner: "cima", acq: "pulseqonline", run: 1,
        osf_id: "6a8c4a86ee75b6b40871e688", bytes: 101155349, sha256: "df6b5aeb977de936ed7617961c1c88d3a6d5c7599289db367f8851de63652680" },
    Example { id: "cima-pulseq-online-run2", scanner: "cima", acq: "pulseqonline", run: 2,
        osf_id: "6a8c4ac730a739d7deff1faf", bytes: 101240462, sha256: "5e5f53544f106e0e34b3004397882130f67759ccd9448e8db557b09b29f5aa21" },
    Example { id: "cima-pulseq-online-run3", scanner: "cima", acq: "pulseqonline", run: 3,
        osf_id: "6a8c4aebee95d1176a71e677", bytes: 100694884, sha256: "cd52f96b2475bc17238c9338bf0f2afbad189f6d457ebd80d3dd1a6e9691bcd7" },
];

/// Look up an example by its registry id.
pub fn find(id: &str) -> Option<&'static Example> {
    EXAMPLES.iter().find(|e| e.id == id)
}

/// The default example. Panics only if [`DEFAULT_ID`] is not in [`EXAMPLES`], which a
/// test pins.
pub fn default_example() -> &'static Example {
    find(DEFAULT_ID).expect("DEFAULT_ID must name a registry entry")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_id_resolves() {
        assert_eq!(default_example().id, DEFAULT_ID);
    }

    #[test]
    fn ids_are_unique() {
        let ids: HashSet<_> = EXAMPLES.iter().map(|e| e.id).collect();
        assert_eq!(ids.len(), EXAMPLES.len());
    }

    #[test]
    fn bids_entities_are_unique_and_alphanumeric() {
        // Every acquisition must land on a distinct ses/acq/run triple, or materializing
        // two of them into one dataset would collide. BIDS labels must be alphanumeric.
        let mut keys = HashSet::new();
        for e in EXAMPLES {
            assert!(keys.insert((e.scanner, e.acq, e.run)), "duplicate entity key: {}", e.id);
            for label in [e.scanner, e.acq] {
                assert!(
                    label.chars().all(|c| c.is_ascii_alphanumeric()),
                    "non-alphanumeric BIDS label in {}: {label}",
                    e.id
                );
            }
        }
    }

    #[test]
    fn checksums_are_well_formed() {
        for e in EXAMPLES {
            assert_eq!(e.sha256.len(), 64, "{}", e.id);
            assert!(e.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()), "{}", e.id);
            assert!(e.bytes > 0, "{}", e.id);
            assert_eq!(e.osf_id.len(), 24, "{}", e.id);
        }
    }

    #[test]
    fn url_uses_the_public_download_endpoint() {
        assert_eq!(
            default_example().url(),
            "https://osf.io/download/6a8c4b1138a9a828b5ff21a9/"
        );
    }
}
