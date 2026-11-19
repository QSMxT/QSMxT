//! Methods prose for background-field removal.

use super::*;

/// The non-default settings of the selected background-field remover. V-SHARP's are worded by
/// its own sentence.
pub(super) fn bg_removal_changes(config: &PipelineConfig) -> Changed {
    let (b, d) = (&config.bg_removal, BgRemovalConfig::default());
    let mut c = Changed::default();
    match b.algorithm {
        BfAlgorithm::Pdf => c.add_fields(&b.pdf, &d.pdf, &[]),
        BfAlgorithm::Lbv => c.add_fields(&b.lbv, &d.lbv, &[]),
        BfAlgorithm::Ismv => {
            c.add(b.ismv.variant != d.ismv.variant, || match b.ismv.variant {
                IsmvVariant::Qsmjl => "QSM.jl's implementation".to_string(),
                IsmvVariant::Wen2014 => "the authors' implementation".to_string(),
            });
            c.add_fields(&b.ismv, &d.ismv, &["variant", "radius"]);
            // The radius is stated even at its default: it scales with the voxel size.
            c.add(true, || match (b.ismv.radius, b.ismv.variant) {
                (Some(r), _) => format!("SMV radius = {r} mm"),
                (None, IsmvVariant::Wen2014) => "an SMV radius of 6 × the smallest voxel dimension".to_string(),
                (None, IsmvVariant::Qsmjl) => "an SMV radius of 2 × the largest voxel dimension".to_string(),
            });
        }
        BfAlgorithm::Sharp => c.add_fields(&b.sharp, &d.sharp, &[]),
        BfAlgorithm::Resharp => c.add_fields(&b.resharp, &d.resharp, &[]),
        BfAlgorithm::Harperella => {
            c.add_fields(&b.harperella, &d.harperella, &["rim"]);
            c.add(b.harperella.rim != d.harperella.rim, || rim_prose(b.harperella.rim));
        }
        BfAlgorithm::Iharperella => {
            c.add_fields(&b.iharperella, &d.iharperella, &["rim"]);
            c.add(b.iharperella.rim != d.iharperella.rim, || rim_prose(b.iharperella.rim));
        }
        BfAlgorithm::Vsharp | BfAlgorithm::Bfrnet | BfAlgorithm::Iqfm => {}
    }
    c
}

/// Which Laplacian values in HARPERELLA's / iHARPERELLA's boundary rim were set to zero.
pub(super) fn rim_prose(rim: HarperellaRim) -> String {
    match rim {
        HarperellaRim::Whole => "every Laplacian value in the brain's boundary rim set to zero".to_string(),
        HarperellaRim::Sti => "the 20 % largest Laplacian values in the brain's boundary rim set to zero, as in STI Suite".to_string(),
    }
}

/// Display name + citations for a background-field removal algorithm: the catalogue's name, or
/// for the networks QSMxT's, which says what the network does.
pub(super) fn bg_removal_name_cite(alg: BfAlgorithm) -> (&'static str, Vec<Citation>) {
    let name = match alg {
        BfAlgorithm::Bfrnet => "BFRnet (deep-learning background removal)",
        BfAlgorithm::Iqfm => "iQFM (deep-learning joint unwrapping + background removal)",
        _ => catalogue_method(MethodStage::BackgroundRemoval, alg.as_str()).name,
    };
    (name, method(MethodStage::BackgroundRemoval, alg.as_str()))
}
