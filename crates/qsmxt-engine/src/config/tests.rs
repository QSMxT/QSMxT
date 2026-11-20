//! The configuration's tests: loading and migrating settings files, and the dependencies one
//! setting forces on another.

mod selected_toml {
    use super::super::*;
    use crate::enums::{BfAlgorithm, QsmAlgorithm, SeparationAlgorithm};

    #[test]
    fn dl_separation_forces_r2prime() {
        // SUSEP-Net and the network alone consume local field + QSM + R2', so they must force R2'.
        let mut native = PipelineConfig::default();
        native.separation.chi_sepnet.route = ChiSepNetRoute::Native;
        native.separation.algorithm = SeparationAlgorithm::ChiSepNet;
        let mut susep = PipelineConfig::default();
        susep.separation.algorithm = SeparationAlgorithm::SusepNet;
        for mut c in [native, susep] {
            c.pipeline.do_chi_separation = true;
            enforce_separation_dependencies(&mut c);
            assert!(c.pipeline.do_r2primemap, "{:?} must force R2'", c.separation.algorithm);
        }
    }

    /// χ-sepnet's toolbox route estimates its own R2′ from R2*, so it asks for R2* alone, and an
    /// R2′ map stays the user's to ask for. R2′ only ever measured is forced as on any method.
    #[test]
    fn the_toolbox_route_needs_only_r2star() {
        let toolbox = |edit: fn(&mut PipelineConfig)| {
            let mut c = PipelineConfig::default();
            c.pipeline.do_chi_separation = true;
            c.separation.algorithm = SeparationAlgorithm::ChiSepNet;
            edit(&mut c);
            enforce_separation_dependencies(&mut c);
            c.pipeline
        };
        let p = toolbox(|_| {});
        assert!(p.do_r2starmap && !p.do_r2primemap && !p.do_r2map, "{p:?}");
        let p = toolbox(|c| c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet);
        assert!(p.do_r2starmap && !p.do_r2primemap && !p.do_r2map, "{p:?}");
        let p = toolbox(|c| c.pipeline.do_r2primemap = true);
        assert!(p.do_r2primemap && p.do_r2starmap && p.do_r2map, "the user's request stands: {p:?}");
        let p = toolbox(|c| c.separation.r2prime_strategy = R2PrimeStrategy::Mese);
        assert!(p.do_r2primemap && p.do_r2starmap && p.do_r2map, "{p:?}");
        let p = toolbox(|c| c.separation.custom_r2prime_tool = Some("mine".into()));
        assert!(!p.do_r2primemap && !p.do_r2starmap && !p.do_r2map, "a supplied R2′ is used as it is: {p:?}");
    }

    /// χ-sep MEDI's defaults come from qsm-core rather than a copy here, so the config, schema,
    /// TUI, command and methods all follow QSM.rs when it retunes them.
    #[test]
    fn chi_sep_medi_defaults_are_qsm_cores() {
        let ours = PipelineConfig::default().separation.chi_sep_medi;
        let core = qsm_core::separation::ChiSepMediParams::default();
        assert_eq!((ours.lambda, ours.lambda_csf, ours.dr_pos, ours.dr_neg, ours.pad),
                   (core.lambda, core.lambda_csf, core.dr_pos, core.dr_neg, core.pad));
    }

    /// Configs written before `combine`/`refinements` existed must still load, and keep the
    /// union behaviour they were written with.
    #[test]
    fn a_morphology_step_of_zero_is_refused_in_a_config_file() {
        let toml = "[masking]\n[[masking.sections]]\ninput = \"magnitude\"\ngenerator = { op = \"bet\", fractional_intensity = 0.5 }\n\
                    refinements = [{ op = \"erode\", iterations = 0 }]\n";
        let e = PipelineConfig::from_toml(toml).unwrap_err().to_string();
        assert!(e.contains("masking.sections[0].refinements[0]: erode: iterations must be at least 1, got 0"), "{e}");
        assert!(PipelineConfig::from_toml(&toml.replace("iterations = 0", "iterations = 2")).is_ok());
        let e = PipelineConfig::from_toml("[masking]\nrefinements = [{ op = \"close\", radius = 0 }]\n").unwrap_err().to_string();
        assert!(e.contains("masking.refinements[0]: close: radius must be at least 1, got 0"), "{e}");
    }

    #[test]
    fn masking_combine_defaults_to_or_and_round_trips() {
        let legacy: MaskingConfig = toml::from_str("inhomogeneity_correction = true\n").unwrap();
        assert_eq!(legacy.combine, MaskCombine::Or);
        assert!(legacy.refinements.is_empty());

        let mut c = PipelineConfig::default();
        c.masking.combine = MaskCombine::And;
        c.masking.refinements = vec![MaskOp::FillHoles { max_size: 0 }];
        let loaded = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(loaded.masking.combine, MaskCombine::And);
        assert_eq!(loaded.masking.refinements, c.masking.refinements);
    }

    /// The changed settings alone, read back on the defaults, give the config again.
    #[test]
    fn to_toml_changed_writes_only_what_differs_and_roundtrips() {
        let d = PipelineConfig::default();
        assert_eq!(d.to_toml_changed().unwrap().trim(), "schema_version = 2", "nothing changed, only the version written");

        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.delta = 0.22;
        c.bg_removal.algorithm = BfAlgorithm::Pdf;
        c.pipeline.do_swi = true;
        let toml = c.to_toml_changed().unwrap();
        assert!(toml.contains("delta = 0.22") && toml.contains("do_swi = true"), "{toml}");
        assert!(!toml.contains("[inversion.tkd]") && !toml.contains("[masking]"), "only what changed: {toml}");
        assert!(toml.lines().count() < 15, "{toml}");
        let pruned = |c: &PipelineConfig| crate::schema::prune_hidden(c, &Default::default()).to_json().unwrap();
        assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&c));

        // A setting that does not apply is left out: RTS's delta under another inversion.
        let mut hidden = c.clone();
        hidden.inversion.algorithm = QsmAlgorithm::Tkd;
        assert!(!hidden.to_toml_changed().unwrap().contains("delta"));

        // The browser profile reads back the same way: its changes that apply, and nothing else.
        let mut browser = PipelineConfig::browser();
        browser.inversion.algorithm = QsmAlgorithm::Xqsm;
        let toml = browser.to_toml_changed().unwrap();
        assert!(toml.contains("xqsm") && toml.len() * 4 < browser.to_toml().unwrap().len(), "{toml}");
        assert_eq!(PipelineConfig::from_toml(&toml).unwrap().to_json().unwrap(), pruned(&browser));
    }

    #[test]
    fn to_toml_selected_prunes_and_roundtrips() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Rts;
        c.inversion.rts.delta = 0.22; // non-default, must survive
        c.bg_removal.algorithm = BfAlgorithm::Vsharp;
        c.bg_removal.vsharp.threshold = 0.05; // non-default, must survive

        let toml = c.to_toml_selected().unwrap();

        // Only the selected algorithm tables remain.
        assert!(toml.contains("[inversion.rts]"), "selected inversion kept");
        assert!(!toml.contains("[inversion.tkd]"), "unselected inversion pruned");
        assert!(!toml.contains("[inversion.medi]"), "unselected inversion pruned");
        assert!(toml.contains("[bg_removal.vsharp]"), "selected bg kept");
        assert!(!toml.contains("[bg_removal.pdf]"), "unselected bg pruned");
        // Non-algorithm sections are untouched.
        assert!(toml.contains("[masking]"));

        // Round-trips like any qsmxt config: selected values preserved, omitted = defaults.
        let loaded = PipelineConfig::from_toml(&toml).unwrap();
        assert_eq!(loaded.inversion.algorithm, QsmAlgorithm::Rts);
        assert_eq!(loaded.inversion.rts.delta, 0.22);
        assert_eq!(loaded.bg_removal.vsharp.threshold, 0.05);
        // Pruned algorithms came back as their defaults.
        assert_eq!(loaded.inversion.tkd, super::super::TkdConfig::default());
        assert_eq!(loaded.bg_removal.pdf, super::super::PdfConfig::default());
    }
}

mod r2primenet_config {
    use super::super::R2primenetConfig;

    #[test]
    fn a_bad_patch_is_an_error_not_a_panic() {
        let d = R2primenetConfig::default();
        assert!(d.validate().is_ok());
        for bad in [[0, 192, 128], [100, 192, 128]] {
            let c = R2primenetConfig { patch: bad, ..d.clone() };
            assert!(c.validate().unwrap_err().contains("multiple of 16"));
        }
    }

    /// The patch settings' texts say "multiple of N", and their minimum is N: QSM.rs's pooling
    /// multiple for each network.
    #[test]
    fn the_patch_texts_and_minimum_are_qsm_rs_pooling_multiple() {
        for (path, m) in [
            ("separation.r2primenet.patch", qsm_core::relaxometry::POOL_MULTIPLE),
            ("separation.chi_sepnet.patch", qsm_core::separation::chisepnet::POOL_MULTIPLE),
            ("separation.chi_sepnet.frame_patch", qsm_core::separation::chisepnet::POOL_MULTIPLE),
            ("separation.susep_net.patch", qsm_core::separation::susep_net::POOL_MULTIPLE),
        ] {
            let s = crate::schema::settings().iter().find(|s| s.path == path).unwrap();
            assert!(s.tui_help.contains(&format!("multiple of {m}")), "{path}: {}", s.tui_help);
            // A text that states the rule states this multiple.
            for text in [s.help, s.tui_help].into_iter().filter(|t| t.contains("multiple of")) {
                assert!(text.contains(&format!("multiple of {m}")), "{path}: {text}");
            }
            assert_eq!(s.min, Some(m as f64), "{path}");
        }
    }
}

mod r2prime_strategy {
    use crate::config::{enforce_separation_dependencies, PipelineConfig};
    use crate::enums::R2PrimeStrategy;

    /// R2' needs R2* on every route — it is the minuend of R2* - R2 and R2PRIMEnet's only input.
    /// R2 is needed only where R2' can actually be *measured*; forcing a MESE-derived R2 map for a
    /// run that is going to predict R2' instead would be work with nothing to consume it.
    #[test]
    fn only_the_measuring_routes_force_r2() {
        let cfg = |s| {
            let mut c = PipelineConfig::default();
            c.pipeline.do_r2primemap = true;
            c.separation.r2prime_strategy = s;
            enforce_separation_dependencies(&mut c);
            c
        };

        for s in [R2PrimeStrategy::Auto, R2PrimeStrategy::Mese] {
            let c = cfg(s);
            assert!(c.pipeline.do_r2map, "{s} can measure R2', so it needs R2");
            assert!(c.pipeline.do_r2starmap, "{s} needs R2*");
        }

        let c = cfg(R2PrimeStrategy::R2primenet);
        assert!(!c.pipeline.do_r2map, "predicting R2' subtracts nothing, so it needs no R2");
        assert!(c.pipeline.do_r2starmap, "R2* is R2PRIMEnet's input");

        // A supplied R2' map means none of it has to be computed.
        let mut c = PipelineConfig::default();
        c.pipeline.do_r2primemap = true;
        c.separation.custom_r2prime_tool = Some("*".to_string());
        enforce_separation_dependencies(&mut c);
        assert!(!c.pipeline.do_r2map);
        assert!(!c.pipeline.do_r2starmap);
    }

    /// Chi-separation forces R2', which in turn must not drag in R2 on the predicting route.
    #[test]
    fn chi_separation_inherits_the_strategy() {
        let mut c = PipelineConfig::default();
        c.pipeline.do_chi_separation = true;
        c.separation.algorithm = crate::enums::SeparationAlgorithm::ChiSepIlsqr;
        c.separation.r2prime_strategy = R2PrimeStrategy::R2primenet;
        enforce_separation_dependencies(&mut c);
        assert!(c.pipeline.do_r2primemap, "the method consumes R2'");
        assert!(c.pipeline.do_r2starmap);
        assert!(!c.pipeline.do_r2map, "R2PRIMEnet needs no spin-echo R2");
    }

    #[test]
    fn auto_is_the_default() {
        assert_eq!(PipelineConfig::default().separation.r2prime_strategy, R2PrimeStrategy::Auto);
    }
}

mod reference_dependency {
    use crate::config::{enforce_reference_dependencies, PipelineConfig};
    use crate::enums::QsmReference;

    /// A region reference is measured on the parcellation, so asking for one asks for SynthSeg.
    /// Erroring instead would make `--qsm-reference thalamus` an incomplete instruction.
    #[test]
    fn a_region_reference_turns_segmentation_on() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(c.pipeline.do_segmentation);
        assert_eq!(c.qsm.reference, QsmReference::Region, "the reference itself is untouched");

        // A supplied dseg is a parcellation too — no need to compute one.
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");

        // No reconstruction, nothing to reference: segmentation is not forced for its own sake.
        let mut c = PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("thalamus".to_string());
        enforce_reference_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation);
    }

    /// `mean` and `none` force nothing, and a hand-edited config naming no region is not a
    /// reference at all — it falls back rather than referencing to an empty set.
    #[test]
    fn the_other_methods_force_nothing_and_an_empty_region_is_left_for_the_error() {
        for method in [QsmReference::Mean, QsmReference::None] {
            let mut c = PipelineConfig::default();
            c.qsm.reference = method;
            enforce_reference_dependencies(&mut c);
            assert!(!c.pipeline.do_segmentation, "{method} should force nothing");
            assert_eq!(c.qsm.reference, method);
        }

        for empty in [None, Some(String::new()), Some("  ".to_string())] {
            let mut c = PipelineConfig::default();
            c.qsm.reference = QsmReference::Region;
            c.qsm.reference_region = empty.clone();
            enforce_reference_dependencies(&mut c);
            // Not rewritten to the mean: the settings view and the command line refuse it.
            assert_eq!(c.qsm.reference, QsmReference::Region, "{empty:?} is left as it is");
            assert_eq!(c.qsm.reference_region, empty);
            assert!(!c.pipeline.do_segmentation);
            let view = crate::schema::settings_view(&c, &crate::schema::ViewFacts::default());
            let error = view.iter().find(|v| v.path == "qsm.reference_region").and_then(|v| v.error.clone());
            assert!(error.is_some(), "{empty:?} is refused");
        }
    }

    /// One flag carries both fields, so the pair has to collapse back to a single value.
    #[test]
    fn the_spec_round_trips_through_one_value() {
        let mut c = PipelineConfig::default();
        assert_eq!(c.qsm.reference_spec(), "mean");
        c.qsm.reference = QsmReference::None;
        assert_eq!(c.qsm.reference_spec(), "none");
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("left-thalamus,4".to_string());
        assert_eq!(c.qsm.reference_spec(), "left-thalamus,4");
    }

    /// The region has to survive a config file, or `--config` silently drops the reference.
    #[test]
    fn the_region_survives_a_toml_round_trip() {
        let mut c = PipelineConfig::default();
        c.qsm.reference = QsmReference::Region;
        c.qsm.reference_region = Some("ventricles".to_string());
        let back = PipelineConfig::from_toml(&c.to_toml().unwrap()).unwrap();
        assert_eq!(back.qsm.reference, QsmReference::Region);
        assert_eq!(back.qsm.reference_region.as_deref(), Some("ventricles"));

        // And an old config with no region key still loads.
        let old = PipelineConfig::from_toml("[qsm]\nreference = \"mean\"\n").unwrap();
        assert_eq!(old.qsm.reference, QsmReference::Mean);
        assert_eq!(old.qsm.reference_region, None);
    }
}

mod analysis_dependency {
    /// The statistics need something to average over and something to average, so asking for them
    /// forces both — and a supplied input satisfies its own side without forcing a recomputation.
    #[test]
    fn analysis_forces_segmentation_and_qsm() {
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_analysis = true;
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(c.pipeline.do_segmentation);
        assert!(c.pipeline.do_qsm);

        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_analysis = true;
        c.segmentation.custom_dseg_tool = Some("freesurfer".to_string());
        c.separation.custom_qsm_tool = Some("*".to_string());
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(!c.pipeline.do_segmentation, "a supplied dseg needs no SynthSeg run");
        assert!(!c.pipeline.do_qsm, "a supplied Chimap needs no reconstruction");

        // Any map satisfies the map side: the table summarises whatever the run made, so asking
        // for R2* and the statistics must not quietly switch reconstruction back on.
        for other in ["t2star", "r2star", "r2", "r2prime", "chisep"] {
            let mut c = crate::config::PipelineConfig::default();
            c.pipeline.do_qsm = false;
            c.pipeline.do_analysis = true;
            match other {
                "t2star" => c.pipeline.do_t2starmap = true,
                "r2star" => c.pipeline.do_r2starmap = true,
                "r2" => c.pipeline.do_r2map = true,
                "r2prime" => c.pipeline.do_r2primemap = true,
                _ => c.pipeline.do_chi_separation = true,
            }
            crate::config::enforce_analysis_dependencies(&mut c);
            assert!(!c.pipeline.do_qsm, "{other} is a map to summarise — QSM must stay off");
            assert!(c.pipeline.do_segmentation, "{other} still needs something to average over");
        }

        // Segmentation on its own forces nothing: it reads the magnitude, not χ.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_segmentation = true;
        crate::config::enforce_analysis_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm);
    }
}

mod smwi_dependency {
    /// SMWI weights the magnitude by χ, so asking for it without a susceptibility map is a request
    /// that cannot be honoured. Turning it on forces QSM rather than silently producing nothing.
    #[test]
    fn smwi_forces_qsm() {
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_smwi = true;
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(c.pipeline.do_qsm, "SMWI needs a susceptibility map");

        // A bring-your-own Chimap is a susceptibility map too — no need to recompute one.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        c.pipeline.do_smwi = true;
        c.separation.custom_qsm_tool = Some("*".to_string());
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm, "a custom Chimap should not force a reconstruction");

        // And it leaves an ordinary run alone.
        let mut c = crate::config::PipelineConfig::default();
        c.pipeline.do_qsm = false;
        crate::config::enforce_smwi_dependencies(&mut c);
        assert!(!c.pipeline.do_qsm);
    }
}

mod option_field {
    /// `LsqrConfig::residual_weighting` is an `Option` whose `None` means "derive it from B0", and
    /// it deliberately carries no `skip_serializing_if`: that attribute would hide the field from
    /// the param-coverage guard, which reads the serialized keys of a default config and would then
    /// report it as unexposed. The attribute is not needed either — `toml` omits a `None` at table
    /// level on its own, which is what this pins.
    #[test]
    fn an_unset_residual_weighting_round_trips_through_toml() {
        let c = crate::config::PipelineConfig::default();
        assert!(c.inversion.lsqr.residual_weighting.is_none(), "None is the default");
        let t = c.to_toml().expect("TOML serialisation of the default config");
        assert!(!t.contains("residual_weighting"), "an unset Option should not be written: {t}");
        let back = crate::config::PipelineConfig::from_toml(&t).expect("round-trip");
        assert_eq!(back.inversion.lsqr.residual_weighting, None);

        // And a set one survives.
        let mut c2 = c.clone();
        c2.inversion.lsqr.residual_weighting = Some(0.25);
        let back2 = crate::config::PipelineConfig::from_toml(
            &c2.to_toml().expect("serialise")).expect("round-trip");
        assert_eq!(back2.inversion.lsqr.residual_weighting, Some(0.25));
    }
}

mod axial_resampling {
    use super::super::*;

    /// v9.22.0 configs carry `obliquity_threshold` alone, a negative value meaning "off": they keep
    /// their meaning under `axial_resampling`, with a warning; a new config cannot go negative.
    #[test]
    fn legacy_obliquity_thresholds_map_onto_axial_resampling() {
        let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = -1.0\n").unwrap();
        assert!(!c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
        assert_eq!(c.pipeline.obliquity_threshold, DEFAULT_OBLIQUITY_THRESHOLD);
        let (c, w) = PipelineConfig::from_toml_migrating("[pipeline]\nobliquity_threshold = 5\n").unwrap();
        assert!(c.pipeline.axial_resampling && w.len() == 1, "{w:?}");
        assert_eq!(c.pipeline.obliquity_threshold, 5.0);
        // Today's configs: both keys, no warning, round trip.
        let mut d = PipelineConfig::default();
        d.pipeline.axial_resampling = true;
        let (back, w) = PipelineConfig::from_toml_migrating(&d.to_toml().unwrap()).unwrap();
        assert!(w.is_empty() && back.pipeline.axial_resampling);
        let (back, w) = PipelineConfig::from_toml_migrating(&PipelineConfig::default().to_toml().unwrap()).unwrap();
        assert!(w.is_empty() && !back.pipeline.axial_resampling);
        assert!(PipelineConfig::from_toml("[pipeline]\naxial_resampling = true\nobliquity_threshold = -1.0\n").is_err());
        // Nothing said about either: off, at the default threshold.
        let c = PipelineConfig::from_toml("").unwrap();
        assert!(!c.pipeline.axial_resampling);
        assert_eq!(c.pipeline.obliquity_threshold, 10.0);
    }
}

mod removed_settings {
    use super::super::*;

    /// Settings files from earlier QSMxT versions still carry two settings that did nothing, TFI's
    /// `merit` (a setting again since, MERIT, on by default) and QSMART's `sdf_spatial_radius`
    /// (removed). They load, and the old values are ignored.
    #[test]
    fn configs_with_removed_settings_still_load() {
        let old = "[inversion]\nalgorithm = \"tfi\"\n\n[inversion.tfi]\nmerit = true\nprecond = 20.0\n\n\
                   [inversion.qsmart]\nsdf_spatial_radius = 8\nvasc_sphere_radius = 5\n";
        let (c, warnings) = PipelineConfig::from_toml_migrating(old).expect("an old config loads");
        assert!(warnings.len() == 1 && warnings[0].starts_with("inversion.tfi.merit = true had no effect"), "{warnings:?}");
        assert_eq!(c.inversion.algorithm, QsmAlgorithm::Tfi);
        assert_eq!(c.inversion.tfi.precond, 20.0);
        assert_eq!(c.inversion.qsmart.vasc_sphere_radius, 5);
        // Saving it again drops them.
        let saved: toml::Table = toml::from_str(&c.to_toml().unwrap()).unwrap();
        let inversion = saved["inversion"].as_table().unwrap();
        assert_eq!(inversion["tfi"]["merit"].as_bool(), Some(qsm_core::inversion::TfiParams::default().merit));
        assert!(!inversion["qsmart"].as_table().unwrap().contains_key("sdf_spatial_radius"));
    }
}

mod qsmart_inputs {
    use super::super::*;

    /// Vessel sizes given without their units were written in mm and read as mm (said under
    /// QSMART); with their units, or not given at all, they read as written, in voxels by default.
    #[test]
    fn vessel_sizes_without_units_read_as_mm() {
        let old = "[inversion]\nalgorithm = \"qsmart\"\n\n[inversion.qsmart]\nvasc_sphere_radius = 8\nfrangi_scale_max = 6.0\n";
        let (c, warnings) = PipelineConfig::from_toml_migrating(old).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
        assert!(warnings.len() == 1 && warnings[0].contains("vessel_size_units"), "{warnings:?}");
        assert!(!warnings[0].contains("--"), "names settings, not flags");
        // Under another inversion the sizes do nothing, so nothing is said.
        let (c, warnings) = PipelineConfig::from_toml_migrating(&old.replace("qsmart\"", "rts\"")).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Mm);
        assert!(warnings.is_empty());
        // Units given: as written.
        let (c, warnings) = PipelineConfig::from_toml_migrating(&format!("{old}vessel_size_units = \"voxels\"\n")).unwrap();
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
        assert!(warnings.is_empty());
        // No sizes: the defaults, in voxels.
        let (c, _) = PipelineConfig::from_toml_migrating("[inversion]\nalgorithm = \"qsmart\"\n").unwrap();
        assert_eq!(c.inversion.qsmart, QsmartConfig::default());
        assert_eq!(c.inversion.qsmart.vessel_size_units, VesselSizeUnits::Voxels);
    }

    /// Every file QSMxT writes reads back with the units it was written with: the full one, and
    /// the changed-settings one, which carries the units wherever it carries a size.
    #[test]
    fn written_configs_keep_their_vessel_units() {
        let mut c = PipelineConfig::default();
        c.inversion.algorithm = QsmAlgorithm::Qsmart;
        c.inversion.qsmart.vasc_sphere_radius = 6;
        for units in [VesselSizeUnits::Voxels, VesselSizeUnits::Mm] {
            c.inversion.qsmart.vessel_size_units = units;
            for text in [c.to_toml().unwrap(), c.to_toml_changed().unwrap()] {
                let (back, warnings) = PipelineConfig::from_toml_migrating(&text).unwrap();
                assert_eq!(back.inversion.qsmart.vessel_size_units, units, "{text}");
                assert_eq!(back.inversion.qsmart.vasc_sphere_radius, 6);
                assert!(warnings.is_empty(), "{warnings:?}");
            }
        }
        // Without a size, the changed-settings file needs no units.
        let mut d = PipelineConfig::default();
        d.inversion.algorithm = QsmAlgorithm::Qsmart;
        assert!(!d.to_toml_changed().unwrap().contains("vessel_size_units"));
    }

    /// QSMART's echo fit runs with any unwrapping, phase offset removal and bipolar correction;
    /// only a threshold qsm-core refuses is refused.
    #[test]
    fn the_echo_fit_takes_any_unwrapping() {
        let mut fm = FieldMappingConfig { b0_estimation: B0Estimation::QsmartEchofit, ..Default::default() };
        assert_eq!(fm.unwrapping_algorithm, UnwrappingAlgorithm::Romeo);
        assert!(fm.validate().is_ok());
        fm.bipolar_correction = true;
        assert!(fm.validate().is_ok());
        fm.phase_offset_removal = false;
        assert!(fm.validate().is_ok());
        fm.unwrapping_algorithm = UnwrappingAlgorithm::Laplacian;
        fm.laplacian_unmasked = true;
        assert!(fm.validate().is_ok());
        fm.qsmart_fit_threshold = 0.0;
        assert!(fm.validate().is_err());
        fm.qsmart_fit_threshold = 40.0;
        fm.qsmart_fit_threshold_mode = crate::enums::QsmartFitThresholdMode::Percentile;
        fm.qsmart_fit_percentile = 0.0;
        assert!(fm.validate().is_err());
        assert!(FieldMappingConfig::default().validate().is_ok());
    }
}
