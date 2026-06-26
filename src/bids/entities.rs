use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Phase,
    Magnitude,
}

/// BIDS entities extracted from a filename.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BidsEntities {
    pub subject: String,
    pub session: Option<String>,
    pub acquisition: Option<String>,
    pub reconstruction: Option<String>,
    pub inversion: Option<String>,
    pub run: Option<String>,
    pub echo: Option<u32>,
    pub part: Option<Part>,
    pub suffix: String,
}

/// Key that groups echoes belonging to the same acquisition.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AcquisitionKey {
    pub subject: String,
    pub session: Option<String>,
    pub acquisition: Option<String>,
    pub reconstruction: Option<String>,
    pub inversion: Option<String>,
    pub run: Option<String>,
    pub suffix: String,
}

impl fmt::Display for AcquisitionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "sub-{}", self.subject)?;
        if let Some(ses) = &self.session {
            write!(f, "_ses-{}", ses)?;
        }
        if let Some(acq) = &self.acquisition {
            write!(f, "_acq-{}", acq)?;
        }
        if let Some(rec) = &self.reconstruction {
            write!(f, "_rec-{}", rec)?;
        }
        if let Some(inv) = &self.inversion {
            write!(f, "_inv-{}", inv)?;
        }
        if let Some(run) = &self.run {
            write!(f, "_run-{}", run)?;
        }
        write!(f, "_{}", self.suffix)
    }
}

impl BidsEntities {
    pub fn acquisition_key(&self) -> AcquisitionKey {
        AcquisitionKey {
            subject: self.subject.clone(),
            session: self.session.clone(),
            acquisition: self.acquisition.clone(),
            reconstruction: self.reconstruction.clone(),
            inversion: self.inversion.clone(),
            run: self.run.clone(),
            suffix: self.suffix.clone(),
        }
    }
}

impl AcquisitionKey {
    /// Build the BIDS basename (without suffix like _Chimap or extension).
    pub fn basename(&self) -> String {
        let mut name = format!("sub-{}", self.subject);
        if let Some(ses) = &self.session {
            name.push_str(&format!("_ses-{}", ses));
        }
        if let Some(acq) = &self.acquisition {
            name.push_str(&format!("_acq-{}", acq));
        }
        if let Some(rec) = &self.reconstruction {
            name.push_str(&format!("_rec-{}", rec));
        }
        if let Some(inv) = &self.inversion {
            name.push_str(&format!("_inv-{}", inv));
        }
        if let Some(run) = &self.run {
            name.push_str(&format!("_run-{}", run));
        }
        name
    }
}

/// Parse BIDS entities from a NIfTI filename.
///
/// Expects filenames like: `sub-01_ses-pre_acq-gre_run-1_echo-2_part-phase_MEGRE.nii.gz`
pub fn parse_entities(filename: &str) -> Option<BidsEntities> {
    // Strip extension (.nii or .nii.gz)
    let stem = filename
        .strip_suffix(".nii.gz")
        .or_else(|| filename.strip_suffix(".nii"))?;

    // Extract suffix (last underscore-separated segment)
    let last_underscore = stem.rfind('_')?;
    let suffix = &stem[last_underscore + 1..];

    let entity_part = &stem[..last_underscore];

    fn extract_entity(s: &str, key: &str) -> Option<String> {
        let prefix = format!("{}-", key);
        for segment in s.split('_') {
            if let Some(value) = segment.strip_prefix(&prefix) {
                return Some(value.to_string());
            }
        }
        None
    }

    let subject = extract_entity(entity_part, "sub")?;

    let part = extract_entity(entity_part, "part").and_then(|p| match p.as_str() {
        "phase" => Some(Part::Phase),
        "mag" => Some(Part::Magnitude),
        _ => None,
    });

    let echo = extract_entity(entity_part, "echo").and_then(|e| e.parse::<u32>().ok());

    Some(BidsEntities {
        subject,
        session: extract_entity(entity_part, "ses"),
        acquisition: extract_entity(entity_part, "acq"),
        reconstruction: extract_entity(entity_part, "rec"),
        inversion: extract_entity(entity_part, "inv"),
        run: extract_entity(entity_part, "run"),
        echo,
        part,
        suffix: suffix.to_string(),
    })
}

/// Get the JSON sidecar path for a NIfTI file.
/// Returns `None` if the path does not have a `.nii` or `.nii.gz` extension.
pub fn sidecar_path(nifti_path: &Path) -> Option<std::path::PathBuf> {
    let s = nifti_path.to_string_lossy();
    let stem = s
        .strip_suffix(".nii.gz")
        .or_else(|| s.strip_suffix(".nii"))?;
    Some(std::path::PathBuf::from(format!("{}.json", stem)))
}

/// Replace the `part-phase` entity with `part-mag` in a filename.
pub fn phase_to_magnitude_path(phase_path: &Path) -> std::path::PathBuf {
    let s = phase_path.to_string_lossy();
    let replaced = s.replace("_part-phase_", "_part-mag_");
    std::path::PathBuf::from(replaced)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let e = parse_entities("sub-1_echo-1_part-phase_MEGRE.nii").unwrap();
        assert_eq!(e.subject, "1");
        assert_eq!(e.echo, Some(1));
        assert_eq!(e.part, Some(Part::Phase));
        assert_eq!(e.suffix, "MEGRE");
        assert_eq!(e.session, None);
    }

    #[test]
    fn test_parse_full() {
        let e =
            parse_entities("sub-01_ses-pre_acq-gre_rec-mag_inv-1_run-2_echo-3_part-mag_T2starw.nii.gz")
                .unwrap();
        assert_eq!(e.subject, "01");
        assert_eq!(e.session, Some("pre".to_string()));
        assert_eq!(e.acquisition, Some("gre".to_string()));
        assert_eq!(e.reconstruction, Some("mag".to_string()));
        assert_eq!(e.inversion, Some("1".to_string()));
        assert_eq!(e.run, Some("2".to_string()));
        assert_eq!(e.echo, Some(3));
        assert_eq!(e.part, Some(Part::Magnitude));
        assert_eq!(e.suffix, "T2starw");
    }

    #[test]
    fn test_sidecar_path() {
        let p = sidecar_path(Path::new("/data/sub-1_echo-1_part-phase_MEGRE.nii.gz"));
        assert_eq!(
            p,
            Some(std::path::PathBuf::from("/data/sub-1_echo-1_part-phase_MEGRE.json"))
        );
    }

    #[test]
    fn test_sidecar_path_nii() {
        let p = sidecar_path(Path::new("/data/sub-1_MEGRE.nii"));
        assert_eq!(p, Some(std::path::PathBuf::from("/data/sub-1_MEGRE.json")));
    }

    #[test]
    fn test_sidecar_path_non_nifti() {
        assert_eq!(sidecar_path(Path::new("/data/file.json")), None);
    }

    #[test]
    fn test_phase_to_mag() {
        let p = phase_to_magnitude_path(Path::new(
            "/data/sub-1_echo-1_part-phase_MEGRE.nii.gz",
        ));
        assert_eq!(
            p,
            std::path::PathBuf::from("/data/sub-1_echo-1_part-mag_MEGRE.nii.gz")
        );
    }

    #[test]
    fn test_acquisition_key_display() {
        let e = parse_entities("sub-01_ses-pre_acq-gre_run-1_echo-2_part-phase_T2starw.nii.gz")
            .unwrap();
        let key = e.acquisition_key();
        assert_eq!(key.to_string(), "sub-01_ses-pre_acq-gre_run-1_T2starw");
    }
}
