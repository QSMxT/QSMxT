use std::fs;

fn main() {
    // Extract qsm-core git hash from Cargo.lock
    let lock = fs::read_to_string("Cargo.lock").unwrap_or_default();
    let mut qsm_core_hash = String::from("unknown");
    let mut in_qsm_core = false;
    for line in lock.lines() {
        if line.trim() == "name = \"qsm-core\"" {
            in_qsm_core = true;
        } else if in_qsm_core && line.starts_with("source = ") {
            // source = "git+https://...?tag=v0.10.0#<hash>"
            if let Some(hash) = line.rsplit('#').next() {
                let hash = hash.trim_end_matches('"');
                qsm_core_hash = hash[..hash.len().min(12)].to_string();
            }
            break;
        } else if line.starts_with("[[package]]") && in_qsm_core {
            break;
        }
    }
    println!("cargo:rustc-env=QSM_CORE_GIT_HASH={}", qsm_core_hash);

    // Extract qsm-core tag from Cargo.toml
    let toml = fs::read_to_string("Cargo.toml").unwrap_or_default();
    let mut qsm_core_version = String::from("unknown");
    for line in toml.lines() {
        if line.contains("qsm-core") {
            // Look for tag = "v0.10.0" or similar
            if let Some(tag_start) = line.find("tag = \"") {
                let rest = &line[tag_start + 7..];
                if let Some(end) = rest.find('"') {
                    qsm_core_version = rest[..end].to_string();
                }
            }
            break;
        }
    }
    println!("cargo:rustc-env=QSM_CORE_VERSION={}", qsm_core_version);

    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
}
