use super::*;
use crate::plan::{plan, tests::facts};
use crate::scan::ScanFacts;

/// A spread of configurations: the defaults, every inversion and background removal, two-pass
/// with each kind of reconstruction, single and multi-echo field mapping, each output switch,
/// separation, segmentation and analysis, referencing, masking recipes, the grid switches and
/// multi-orientation.
fn configs() -> Vec<(String, PipelineConfig)> {
    let mut out: Vec<(String, PipelineConfig)> = Vec::new();
    let mut add = |name: String, f: &dyn Fn(&mut PipelineConfig)| {
        let mut c = PipelineConfig::default();
        f(&mut c);
        crate::config::enforce_separation_dependencies(&mut c);
        out.push((name, c));
    };
    add("default".into(), &|_| {});
    add("browser".into(), &|c| *c = PipelineConfig::browser());
    for alg in QsmAlgorithm::ALL {
        add(format!("inversion={alg}"), &|c| c.inversion.algorithm = alg);
        add(format!("inversion={alg}+two-pass"), &|c| {
            c.inversion.algorithm = alg;
            c.masking.two_pass = true;
        });
        add(format!("inversion={alg}+laplacian"), &|c| {
            c.inversion.algorithm = alg;
            c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        });
    }
    for alg in [BfAlgorithm::Vsharp, BfAlgorithm::Pdf, BfAlgorithm::Lbv, BfAlgorithm::Ismv, BfAlgorithm::Sharp,
                BfAlgorithm::Resharp, BfAlgorithm::Harperella, BfAlgorithm::Iharperella, BfAlgorithm::Bfrnet,
                BfAlgorithm::Iqfm] {
        add(format!("bgremove={alg}"), &|c| c.bg_removal.algorithm = alg);
        add(format!("bgremove={alg}+msmv+two-pass"), &|c| {
            c.bg_removal.algorithm = alg;
            c.bg_removal.msmv_refine = true;
            c.masking.two_pass = true;
        });
    }
    for grid in [VsharpGrid::Finest, VsharpGrid::Isotropic, VsharpGrid::Native] {
        add(format!("vsharp-grid={grid:?}"), &|c| c.bg_removal.vsharp.grid = grid);
    }
    add("vsharp-deconvolve".into(), &|c| {
        c.bg_removal.vsharp.deconvolve = !c.bg_removal.vsharp.deconvolve;
        c.bg_removal.vsharp.max_radius = 8.0;
    });
    add("tiles".into(), &|c| {
        c.inversion.algorithm = QsmAlgorithm::Xqsm;
        c.inversion.tile_size = Some(64);
        c.inversion.tile_halo = Some(4);
    });
    for b0 in [B0Estimation::WeightedAvg, B0Estimation::LinearFit, B0Estimation::RobustFit] {
        add(format!("b0={b0:?}"), &|c| c.field_mapping.b0_estimation = b0);
    }
    add("b0-assumed-decay".into(), &|c| c.field_mapping.b0_weight_type = B0WeightType::AssumedDecay);
    add("robust-fit-0".into(), &|c| {
        c.field_mapping.b0_estimation = B0Estimation::RobustFit;
        c.field_mapping.robust_fit.iterations = 0;
    });
    add("laplacian-fft".into(), &|c| {
        c.field_mapping.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        c.field_mapping.laplacian_solver = LaplacianSolver::Fft;
        c.field_mapping.laplacian_fft_pad = [8, 8, 4];
    });
    add("romeo-individual".into(), &|c| c.field_mapping.romeo.individual = !c.field_mapping.romeo.individual);
    for (offsets, bipolar) in [(false, false), (true, true), (false, true)] {
        add(format!("offsets={offsets},bipolar={bipolar}"), &|c| {
            c.field_mapping.phase_offset_removal = offsets;
            c.field_mapping.bipolar_correction = bipolar;
        });
    }
    add("motion".into(), &|c| c.motion_correction.enabled = true);
    add("outputs".into(), &|c| {
        c.pipeline.do_swi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        c.pipeline.do_smwi = true;
    });
    add("t2star".into(), &|c| c.pipeline.do_t2starmap = true);
    add("r2star".into(), &|c| c.pipeline.do_r2starmap = true);
    add("swi-only".into(), &|c| {
        c.pipeline.do_qsm = false;
        c.pipeline.do_swi = true;
    });
    add("swi-changed".into(), &|c| {
        c.pipeline.do_swi = true;
        c.swi.unwrap = SwiUnwrap::Romeo;
        c.swi.mip_window = 9;
        c.swi.strength = 2.0;
    });
    add("smwi-changed".into(), &|c| {
        c.pipeline.do_smwi = true;
        c.smwi.threshold_ppm = 0.2;
        c.smwi.mip_window = 5;
    });
    add("no-qsm".into(), &|c| {
        c.pipeline.do_qsm = false;
        c.pipeline.do_r2starmap = true;
    });
    for alg in [SeparationAlgorithm::R2starQsm, SeparationAlgorithm::Decompose, SeparationAlgorithm::ChiSepIlsqr,
                SeparationAlgorithm::ChiSepMedi, SeparationAlgorithm::WaveSep, SeparationAlgorithm::HcChisep,
                SeparationAlgorithm::SusepNet, SeparationAlgorithm::ChiSepNet] {
        add(format!("separation={alg}"), &|c| {
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = alg;
        });
    }
    for s in [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese, R2PrimeStrategy::R2primenet] {
        add(format!("r2prime={s:?}"), &|c| {
            c.pipeline.do_r2primemap = true;
            c.pipeline.do_r2map = true;
            c.separation.r2prime_strategy = s;
        });
    }
    add("r2prime-any-field".into(), &|c| {
        c.pipeline.do_r2primemap = true;
        c.separation.r2primenet.ignore_field_mismatch = true;
    });
    add("r2prime-supplied".into(), &|c| {
        c.pipeline.do_r2primemap = true;
        c.separation.custom_r2prime_tool = Some("sepia".into());
    });
    add("segmentation".into(), &|c| c.pipeline.do_segmentation = true);
    add("segmentation+analysis".into(), &|c| {
        c.pipeline.do_segmentation = true;
        c.pipeline.do_analysis = true;
    });
    add("reference=none".into(), &|c| c.qsm.reference = QsmReference::None);
    add("reference=region".into(), &|c| {
        c.pipeline.do_segmentation = true;
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("lateral-ventricles,csf".into());
    });
    add("reference=region+two-pass".into(), &|c| {
        c.masking.two_pass = true;
        c.pipeline.do_segmentation = true;
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("csf".into());
    });
    for (name, recipe) in mask_presets() {
        add(format!("mask={name}"), &|c| {
            c.masking.sections = recipe.sections.clone();
            c.masking.combine = recipe.combine;
            c.masking.refinements = recipe.refinements.clone();
        });
    }
    add("mask-inhomogeneity".into(), &|c| c.masking.inhomogeneity_correction = true);
    add("mask-supplied".into(), &|c| c.masking.custom_mask_tool = Some("qsmbly".into()));
    add("mask-supplied-any".into(), &|c| c.masking.custom_mask_tool = Some("*".into()));
    add("mask-none".into(), &|c| c.masking.sections.clear());
    add("grid".into(), &|c| {
        c.pipeline.ignore_slice_gap = true;
        c.pipeline.axial_resampling = true;
        c.pipeline.crop_to_mask = true;
        c.pipeline.fft_padding = true;
    });
    add("grid-working".into(), &|c| {
        c.pipeline.axial_resampling = true;
        c.pipeline.output_space = OutputSpace::Working;
        c.pipeline.obliquity_threshold = 10.0;
    });
    add("cosmos".into(), &|c| {
        c.multi_orientation.group_by = "acq".into();
        c.multi_orientation.magnitude_weighting = true;
    });
    add("sti".into(), &|c| {
        c.multi_orientation.group_by = "acq".into();
        c.multi_orientation.algorithm = MultiOrientAlgorithm::Sti;
        c.multi_orientation.register = false;
        c.multi_orientation.lambda = 1e-3;
    });
    add("dicom".into(), &|c| c.pipeline.export_dicom = true);
    add("everything".into(), &|c| {
        c.masking.two_pass = true;
        c.masking.inhomogeneity_correction = true;
        c.motion_correction.enabled = true;
        c.pipeline.do_swi = true;
        c.pipeline.do_smwi = true;
        c.pipeline.do_t2starmap = true;
        c.pipeline.do_r2starmap = true;
        c.pipeline.do_r2map = true;
        c.pipeline.do_r2primemap = true;
        c.pipeline.do_chi_separation = true;
        c.pipeline.do_segmentation = true;
        c.pipeline.do_analysis = true;
        c.pipeline.crop_to_mask = true;
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("csf".into());
    });
    out
}

/// The scans each configuration is planned on: the example (five echoes), one echo, and one
/// without magnitude.
fn scans() -> Vec<(&'static str, ScanFacts)> {
    let mut single = facts();
    single.n_echoes = 1;
    single.echo_times.truncate(1);
    let mut phase_only = facts();
    phase_only.has_magnitude = false;
    let mut supplied = facts();
    supplied.custom_mask = true;
    supplied.custom_r2prime = true;
    vec![("multi-echo", facts()), ("single-echo", single), ("phase-only", phase_only), ("supplied", supplied)]
}

/// The text is a join over the structured form, whatever the scan: [`generate_methods_for`]'s
/// Markdown is [`Methods::to_markdown`], the paragraph is its sentences, and every work the
/// reference list holds is cited by a sentence, in the order the sentences first cite them.
#[test]
fn the_text_is_the_sentences_joined() {
    for (name, c) in configs() {
        for tool in ["qsmxt", "QSMbly"] {
            let text = generate_methods_for(&c, tool);
            for (scan, f) in scans() {
                let m = methods_structured(&c, &plan(&c, &f), tool);
                assert_eq!(m.to_markdown(), text, "{name} on {scan} [{tool}]");
                let paragraph = text.split("\n\n").nth(1).unwrap();
                assert_eq!(m.paragraph(), paragraph, "{name}");
                let mut cited: Vec<&str> = Vec::new();
                for s in &m.sentences {
                    assert!(!s.text.is_empty() && s.text.trim() == s.text, "{name}: {:?}", s.text);
                    for k in &s.citations {
                        assert!(m.references.iter().any(|r| &r.key == k), "{name}: {k} cited but not listed");
                        if !cited.contains(&k.as_str()) {
                            cited.push(k);
                        }
                    }
                }
                let listed: Vec<&str> = m.references.iter().map(|r| r.key.as_str()).collect();
                assert_eq!(cited, listed, "{name}: the references, as the sentences cite them");
            }
        }
    }
}

/// Every step a sentence names is a box of the pipeline graph of the same configuration and
/// scan. The steps no sentence describes are printed (`--nocapture`), not failed on: some steps
/// (loading the scan, scaling the phase) are not methods.
#[test]
fn sentences_name_steps_of_the_graph() {
    let mut undescribed: Vec<String> = Vec::new();
    for (name, c) in configs() {
        for (scan, f) in scans() {
            let plan = plan(&c, &f);
            let graph = crate::pipeline_graph::pipeline_graph(&plan, &c, &f, None);
            let m = methods_structured(&c, &plan, "QSMbly");
            for s in &m.sentences {
                for id in &s.steps {
                    assert!(graph.node(id).is_some(), "{name} on {scan}: {id} is not in the graph: {:?}", s.text);
                }
            }
            for n in &graph.nodes {
                if !m.sentences.iter().any(|s| s.steps.contains(&n.id)) && !undescribed.contains(&n.id) {
                    undescribed.push(n.id.clone());
                }
            }
        }
    }
    eprintln!("steps no sentence describes in some configuration: {}", undescribed.join(", "));
}

/// What a few sentences name.
#[test]
fn sentences_name_the_steps_they_describe() {
    let steps = |c: &PipelineConfig, f: &ScanFacts, start: &str| -> Vec<String> {
        let m = methods_structured(c, &plan(c, f), "QSMbly");
        let s = m.sentences.iter().find(|s| s.text.starts_with(start)).unwrap_or_else(|| panic!("{start}: {m:#?}"));
        s.steps.clone()
    };
    let mut c = PipelineConfig::default();
    assert_eq!(steps(&c, &facts(), "QSM processing"), Vec::<String>::new());
    assert_eq!(steps(&c, &facts(), "A brain mask"), ["mask"]);
    assert_eq!(steps(&c, &facts(), "Phase unwrapping"), ["unwrap"]);
    assert_eq!(steps(&c, &facts(), "Background field removal"), ["bgremove"]);
    assert_eq!(steps(&c, &facts(), "Dipole inversion"), ["invert"]);
    assert_eq!(steps(&c, &facts(), "Susceptibility values were referenced"), ["reference"]);

    // Two-pass: the per-pass sentences name both passes' boxes; the two-pass sentence the second
    // pass and the combination.
    c.masking.two_pass = true;
    assert_eq!(steps(&c, &facts(), "Dipole inversion"), ["invert", "invert-reliable"]);
    assert_eq!(steps(&c, &facts(), "Two-pass"), ["mask-reliable", "bgremove-reliable", "invert-reliable", "twopass"]);
    assert_eq!(steps(&c, &facts(), "Susceptibility values were referenced"), ["reference", "reference-singlepass"]);

    // TGV does the background removal and the inversion in one box.
    let mut c = PipelineConfig::default();
    c.inversion.algorithm = QsmAlgorithm::Tgv;
    assert_eq!(steps(&c, &facts(), "QSM was computed using the Total Generalized Variation"), ["tgv"]);

    // Motion correction is a box only where it runs (several echoes, with magnitude).
    c.motion_correction.enabled = true;
    assert_eq!(steps(&c, &facts(), "Before field mapping"), ["motion"]);
    let mut single = facts();
    single.n_echoes = 1;
    single.echo_times.truncate(1);
    assert_eq!(steps(&c, &single, "Before field mapping"), Vec::<String>::new());

    // The citations go with the sentence that names the work.
    let m = methods_structured(&c, &plan(&c, &facts()), "QSMbly");
    let tgv = m.sentences.iter().find(|s| s.text.contains("(TGV)")).unwrap();
    assert_eq!(tgv.citations, ["langkammer2015"]);
}
