//! The methods paragraph in the order the pipeline runs: [`draft`] calls each stage's prose and
//! still words the later stages (segmentation, SWI/SMWI, relaxometry, separation,
//! multi-orientation, DICOM export) inline.

use super::*;

pub(super) fn draft(config: &PipelineConfig, tool: &str, ran: Ran) -> Draft {
    let mut doc = Draft::default();

    let (cite, author_year) = match tool {
        "QSMbly" | "qsmbly" => (&CITE_QSMBLY, "Stewart, 2026"),
        _ => (&CITE_QSMXT, "Stewart, 2026"),
    };

    doc.cite(cite);
    if config.pipeline.do_qsm {
        doc.say(&[], format!("QSM processing was performed using {} ({}).", tool, author_year));
    } else {
        doc.say(&[], format!("MRI processing was performed using {} ({}).", tool, author_year));
    }
    // The toolbox pipeline the settings were chosen from, while its choices hold; the settings
    // below say where they depart from it.
    if let Some(p) = config.reference_pipeline.as_deref()
        .filter(|id| crate::pipelines::chosen(config, id))
        .and_then(qsm_core::pipeline::reference_pipelines::reference_pipeline) {
        doc.say(&[], format!("Processing followed the {} pipeline.", p.toolbox));
        // UK Biobank's brain mask comes from its T1-weighted images, which QSMxT does not use.
        if p.id == "uk-biobank" {
            doc.say(&[], "The brain mask was made from the gradient-echo images as described below, \
                in place of the UK Biobank pipeline's mask from its T1-weighted images.".to_string());
        }
    }

    // The k-space Tukey window comes before every other step.
    if config.pipeline.tukey_window > 0.0 {
        doc.say(&[Stage::TukeyWindow], format!(
            "Before any other processing, each echo's complex image was multiplied in k-space by a Tukey \
             window with a taper ratio of {}.", config.pipeline.tukey_window));
    }

    // Masking (inhomogeneity correction is described inline)
    describe_masking(config, ran, &mut doc);

    // QSM-specific pipeline
    if config.pipeline.do_qsm {
        describe_reconstruction_grid(config, ran, &mut doc);
        match config.inversion.algorithm {
            QsmAlgorithm::Tgv => {
                // Routed by QSM.rs's `InversionInput`: one echo goes in as wrapped phase (TGV
                // unwraps it itself), several are field-mapped first and go in as the total field.
                // Neither route runs a separate background-removal stage.
                doc.cite(&CITE_TGV);
                doc.say(&both(Stage::Tgv), format!(
                    "QSM was computed using the Total Generalized Variation (TGV) algorithm \
                     (Langkammer et al., 2015), with background field removal and dipole inversion \
                     performed jointly. For single-echo data TGV was applied to the wrapped phase of \
                     the echo and also performed the phase unwrapping; for multi-echo data TGV was \
                     applied to the B0 field map, computed as described below, expressed as phase at \
                     {}. The regularisation weights are in radians at that echo time \
                     and the field strength, and the susceptibility was converted to ppm with them.",
                    echo_time_prose(config.inversion.tgv.te)));
                let changed = inversion_changes(config, QsmAlgorithm::Tgv);
                if !changed.is_empty() {
                    doc.say(&both(Stage::Tgv), format!("TGV was run{}.", changed.with()));
                }
                describe_field_mapping(config, ran, &mut doc);
            }
            QsmAlgorithm::Iqsm | QsmAlgorithm::IqsmPlus => {
                // `InversionInput::WrappedPhase`: no field mapping and no background removal ran.
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                doc.cite(cite);
                doc.say(&both(Stage::Inversion), format!(
                    "QSM was reconstructed with {} ({}) from the wrapped phase of each echo, with \
                     phase unwrapping, background field removal and dipole inversion performed by the \
                     network.",
                    name, cite_inline(cite),
                ));
                say_tiling(config, Stage::Inversion, &mut doc);
            }
            QsmAlgorithm::Qsmart => {
                // QSMART consumes a pre-computed total field, so the field-mapping
                // stage (phase offset, unwrapping, B0 estimation) still runs and must
                // be described.
                describe_field_mapping(config, ran, &mut doc);
                let (inv_name, inv_cite) = inversion_name_cite(config.inversion.qsmart.inversion);
                doc.cite(&CITE_QSMART);
                doc.cite(inv_cite);
                doc.say(&both(Stage::Qsmart), format!(
                    "QSM was computed using QSMART (Yaghmaie et al., 2021), with spatially dependent \
                     filtering for background field removal, {} dipole inversion ({}), and Frangi \
                     vesselness-based tissue/vasculature separation{}.",
                    inv_name, cite_inline(inv_cite), qsmart_changes(config).comma_with(),
                ));
                // The magnitude vessels were detected on.
                match config.inversion.qsmart.vessel_magnitude {
                    QsmartVesselMagnitude::MeanN4 => {
                        doc.cite(&CITE_N4);
                        doc.say(&both(Stage::Qsmart), "Vessels were detected on the mean magnitude over echoes, \
                            bias-corrected with N4 (Tustison et al., 2010).".to_string());
                    }
                    QsmartVesselMagnitude::Combined => {
                        doc.say(&both(Stage::Qsmart), format!("Vessels were detected on the {}RSS-combined magnitude.",
                            if config.masking.inhomogeneity_correction { "inhomogeneity-corrected " } else { "" }));
                    }
                }
            }
            _ => {
                describe_field_mapping(config, ran, &mut doc);

                // Background removal — only in front of a local-field inversion (QSM.rs
                // `InversionInput`); the total-field methods remove or model it themselves.
                let total_field = !crate::bridge::inversion_input(config, 2).needs_bg_removal();
                if !total_field {
                    let alg = &config.bg_removal.algorithm;
                    let (name, cite) = bg_removal_name_cite(*alg);
                    doc.cite(cite);
                    let erode = config.bg_removal.erode_voxels;
                    if erode > 0 {
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before background field removal, the brain mask was eroded by {erode} voxel{}.",
                            if erode == 1 { "" } else { "s" }));
                    }
                    if config.bg_removal.ukb_mask_trim {
                        doc.cite(&CITE_UKB_QSM);
                        doc.say(&both(Stage::BackgroundRemoval), format!(
                            "Before and after background field removal, the mask was trimmed of voxels with \
                             unreliable phase as in the UK Biobank pipeline ({}): phase-reliability maps of the \
                             first and last echoes (the magnitude of the unit phasor averaged over a 2 mm ball) \
                             were thresholded at 0.6 and 0.5 before background removal and at 0.7 and 0.6 after \
                             it, then in each slice components smaller than 300 (after: 200) voxels were removed \
                             and holes smaller than 50 (after: 30) voxels filled, and enclosed holes were filled.",
                            cite_inline(&CITE_UKB_QSM)));
                    }
                    if config.bg_removal.polish_mask {
                        doc.say(&both(Stage::BackgroundRemoval),
                            "After background field removal, the mask was reduced to its largest connected \
                             component with its holes filled, and the local field was set to zero outside it."
                                .to_string());
                    }
                    if *alg == BfAlgorithm::Vsharp {
                        let (v, d) = (&config.bg_removal.vsharp, VsharpConfig::default());
                        let mut changed = Changed::default();
                        changed.add(differs(v.max_radius, d.max_radius) || differs(v.min_radius, d.min_radius)
                            || differs(v.radius_step, d.radius_step),
                            || format!("SMV kernel radii from {} to {} mm in {} mm steps", v.max_radius, v.min_radius, v.radius_step));
                        changed.add(v.deconvolve != d.deconvolve, || format!(
                            "truncated-SVD deconvolution {}",
                            if v.deconvolve { "enabled" } else { "disabled" }));
                        // The threshold only exists for the deconvolution.
                        changed.add(v.deconvolve && differs(v.threshold, d.threshold),
                            || format!("a deconvolution threshold of {}", v.threshold));
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), changed.with()));
                        let resample = "the field and mask were cropped to the brain mask's bounding box and resampled trilinearly, and the background field estimated there was interpolated back to the acquired grid, so the local field kept the acquired resolution";
                        match v.grid {
                            VsharpGrid::Finest => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "For anisotropic voxels the V-SHARP background was estimated on an isotropic grid at the smallest acquired voxel dimension: {resample}. Isotropic acquisitions were processed on their acquired grid.")),
                            VsharpGrid::Isotropic if !differs(v.grid_spacing, d.grid_spacing) => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on an isotropic grid: {resample}.")),
                            VsharpGrid::Isotropic => doc.say(&both(Stage::BackgroundRemoval), format!(
                                "The V-SHARP background was estimated on a {} mm isotropic grid: {resample}.", v.grid_spacing)),
                            VsharpGrid::Native => {}
                        }
                    } else {
                        doc.say(&both(Stage::BackgroundRemoval), format!("Background field removal was performed using {} ({}){}.",
                            name, cite_inline(cite), bg_removal_changes(config).with()));
                    }

                }

                // Dipole inversion
                let (name, cite) = inversion_name_cite(config.inversion.algorithm);
                let target = if total_field { " on the total field within the brain mask" } else { "" };
                doc.cite(cite);
                let msmv = medi_msmv(config, &mut doc) + &medi_smv(config);
                let changes = inversion_changes(config, config.inversion.algorithm);
                doc.say(&both(Stage::Inversion), format!("Dipole inversion was performed{} using {} ({}){msmv}{}.",
                    target, name, cite_inline(cite),
                    if msmv.is_empty() { changes.with() } else { changes.comma_with() }));
                describe_medi_csf(config, ran, &mut doc);
                // Deep-learning tiling (memory-bounded, approximate) is noted when enabled — only
                // for the tileable networks (a stray --tile-size on a classical algorithm is ignored).
                if config.inversion.algorithm.tileable() {
                    say_tiling(config, Stage::Inversion, &mut doc);
                } else if config.runs_a_tileable_network() {
                    say_tiling(config, Stage::BackgroundRemoval, &mut doc);
                }
            }
        }

        // Two-pass wraps background removal and inversion, so it is described after both and
        // before referencing, which runs on the combined map.
        describe_two_pass(config, ran, &mut doc);
        // HEIDI after the inversion, and after two-pass on the combined map.
        describe_heidi(config, ran, &mut doc);

        // Referencing
        match config.qsm.reference {
            QsmReference::Mean => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility within \
                     {RECON_MASK}."));
            }
            QsmReference::None => {
                doc.say(&[Stage::Reference, Stage::SinglePassReference], "No susceptibility referencing was applied.".to_string());
            }
            QsmReference::Region => {
                let region = config.qsm.reference_region.as_deref().unwrap_or("");
                let pretty = region.replace('-', " ").replace(',', ", ");
                let plural = if region.contains(',') { "s" } else { "" };
                let labels = match config.segmentation.custom_dseg_tool.as_deref() {
                    Some(tool) => format!("'{tool}' segmentation label{plural}"),
                    None => format!("SynthSeg label{plural}"),
                };
                doc.say(&[Stage::Reference, Stage::SinglePassReference], format!(
                    "Susceptibility values were referenced to the mean susceptibility in the \
                     {pretty} ({labels}), computed within {RECON_MASK}."
                ));
            }
        }
        if says_per_echo(config) {
            doc.say(&[Stage::Reference],
                "For multi-echo data, the susceptibility map reported is the mean across echoes of the \
                 echoes' maps, over the voxels where every echo's map was defined; each echo's map is \
                 reported too.".to_string());
        }
    }

    // A supplied segmentation, every run's or some runs'.
    let dseg_tool = config.segmentation.custom_dseg_tool.as_deref();
    let dseg_supplied = ran.count(Topic::SegmentationSource, Choice::Supplied) > 0;
    let dseg_all_supplied = ran.only(Topic::SegmentationSource, &[Choice::Supplied]);
    if config.pipeline.do_segmentation && dseg_supplied {
        let from = match dseg_tool { Some(t) if t != "*" => format!("'{t}' "), _ => String::new() };
        doc.say(&[Stage::Segmentation], if dseg_all_supplied {
            format!("Segmentations were sourced from the {from}BIDS derivatives.")
        } else {
            format!("Where available, segmentations were sourced from the {from}BIDS derivatives; \
                     otherwise they were generated as follows.")
        });
    }
    if config.pipeline.do_segmentation && !dseg_all_supplied && !ran.none_ran(Stage::Segmentation) {
        doc.cite(&CITE_SYNTHSEG);
        let v2 = config.segmentation.version == SynthSegVersion::V2;
        if v2 {
            doc.cite(&CITE_SYNTHSEG2);
        }
        // The label table counts the background.
        let structures = config.segmentation.version.to_core().n_labels() - 1;
        let (s, d) = (&config.segmentation, SegmentationConfig::default());
        let mut changed = Changed::default();
        changed.add(s.flip_averaging != d.flip_averaging, || format!(
            "left-right flip averaging of the posteriors {}",
            if s.flip_averaging { "enabled" } else { "disabled" }));
        // The version names the weights (and the label count says what they produce); the
        // bring-your-own tool is not a SynthSeg setting.
        if let Some(patch) = s.patch {
            changed.add(true, || format!(
                "the network applied as a sliding window of {p}×{p}×{p}-voxel patches overlapping by at \
                 least half a patch, their posteriors Gaussian-weighted", p = patch.div_ceil(16) * 16));
        }
        changed.add_fields(s, &d, &["version", "flip_averaging", "custom_dseg_tool", "patch"]);
        doc.say(&[Stage::Segmentation], format!(
            "The brain was parcellated from the GRE magnitude using SynthSeg {} ({}), yielding {} \
             FreeSurfer-labelled structures{}.",
            config.segmentation.version,
            if v2 { "Billot, Greve, et al., 2023; Billot, Magdamo, et al., 2023" } else { "Billot et al., 2023" },
            structures, changed.comma_with(),
        ));
    }
    if config.pipeline.do_analysis && !ran.none_ran(Stage::Analysis) {
        doc.say(&[Stage::Analysis], 
            "Each quantitative map was then summarised within every segmented structure (mean, \
             standard deviation, median, minimum, maximum and 5th/95th percentiles over the \
             structure's voxels), excluding voxels the map does not cover.".to_string());
    }

    if config.pipeline.do_smwi && !ran.none_ran(Stage::Smwi) {
        doc.cite(&CITE_SMWI);
        let (s, d) = (&config.smwi, SmwiConfig::default());
        let mut mask = Changed::default();
        mask.add(differs(s.threshold_ppm, d.threshold_ppm), || format!("|χ| threshold {:.2} ppm", s.threshold_ppm));
        mask.add(differs(s.power, d.power), || format!("mask raised to the power {}", s.power));
        let slab = if s.mip_window != d.mip_window { format!(" over {} slices", s.mip_window) } else { String::new() };
        doc.say(&[Stage::Smwi], format!(
            "Susceptibility map-weighted images were computed (Gho et al., 2014) by weighting the \
             magnitude with a mask derived from the susceptibility map{}, separately for \
             paramagnetic and diamagnetic sources, each followed by a minimum-intensity \
             projection{slab}.",
            mask.paren(),
        ));
    }

    // SWI
    if config.pipeline.do_swi {
        let (swi, d) = (&config.swi, SwiConfig::default());
        let mut changed: Vec<String> = Vec::new();
        match swi.unwrap {
            _ if swi.unwrap == d.unwrap => {}
            SwiUnwrap::Romeo => {
                changed.push("ROMEO phase unwrapping (Dymerska et al., 2021)".into());
                doc.cite(&CITE_ROMEO);
            }
            SwiUnwrap::LaplacianSlice => changed.push("slice-wise Laplacian phase unwrapping".into()),
            SwiUnwrap::Laplacian => changed.push("Laplacian phase unwrapping".into()),
        }
        if swi.hp_sigma.iter().zip(d.hp_sigma).any(|(a, b)| differs(*a, b)) {
            changed.push(format!(
                "a high-pass filter σ of {} voxels",
                swi.hp_sigma.iter().map(|v| format!("{v}")).collect::<Vec<_>>().join(" × "),
            ));
        }
        if swi.scaling != d.scaling {
            changed.push(format!("{} phase-mask scaling", swi.scaling));
        }
        if differs(swi.strength, d.strength) {
            changed.push(format!("phase-mask strength {}", swi.strength));
        }
        let mip = if swi.mip_window != d.mip_window {
            format!("Minimum-intensity projections were taken over {}-slice slabs.", swi.mip_window)
        } else {
            "Minimum-intensity projections of these images were also computed.".to_string()
        };
        let using = if changed.is_empty() { String::new() } else { format!(" using {}", join_list(&changed)) };
        // ROMEO and slice-wise unwrapping filter in-plane whatever the σ; Laplacian does so when
        // σ has no slice component (the default).
        let phase = if swi.filtered_phase {
            let in_plane = swi.unwrap != SwiUnwrap::Laplacian || swi.hp_sigma[2] == 0.0;
            format!(" The filtered phase that weights these images, CLEAR-SWI's phase after {} Gaussian \
                     high-pass filter within a head mask from the first echo's magnitude, was also saved, in Hz \
                     for multi-echo data and in radians for single-echo data.",
                    if in_plane { "an in-plane" } else { "a" })
        } else {
            String::new()
        };
        doc.cite(&CITE_CLEARSWI);
        doc.say(&[Stage::Swi], format!(
            "Susceptibility-weighted images were computed with CLEAR-SWI (Eckstein et al., 2021){using}. {mip}{phase}"
        ));
    }

    // T2*/R2*
    if config.pipeline.do_t2starmap && config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* and R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_t2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "T2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    } else if config.pipeline.do_r2starmap {
        doc.cite(&CITE_ARLO);
        doc.say(&[Stage::Relaxometry], "R2* maps were computed from multi-echo magnitude data using the ARLO method (Pei et al., 2015).".to_string());
    }

    // R2 / R2'. Worth stating how R2' was obtained, not just that it was: a measured R2' and an
    // estimated one support different claims, and only one of them is a measurement.
    if config.pipeline.do_r2map || config.pipeline.do_r2primemap {
        if config.pipeline.do_r2map && ran.only(Topic::R2Source, &[Choice::Supplied]) {
            doc.say(&[Stage::R2], "R2 maps were sourced from the supplied BIDS derivatives.".to_string());
        } else if config.pipeline.do_r2map && !ran.only(Topic::R2Source, &[Choice::Unavailable]) {
            doc.cite(&CITE_EPG);
            doc.say(&[Stage::R2], 
                "R2 maps were fitted from multi-echo spin-echo magnitude data using an Extended \
                 Phase Graph dictionary (Weigel, 2015).".to_string());
        }
        if config.pipeline.do_r2primemap {
            // R2PRIMEnet's own settings; the field-strength override gets a sentence below.
            let mut r2pn = Changed::default();
            r2pn.add_fields(&config.separation.r2primenet, &R2primenetConfig::default(),
                &["ignore_field_mismatch"]);
            let r2pn = r2pn.semi();
            // What the runs that made an R2′ did, where they all did the same.
            let measured = ran.count(Topic::R2primeSource, Choice::Measured) > 0;
            let predicted = ran.count(Topic::R2primeSource, Choice::Predicted) > 0;
            let none_made = ran.only(Topic::R2primeSource, &[Choice::Unavailable]);
            match config.separation.custom_r2prime_tool.as_deref() {
                _ if none_made => {}
                // `auto` that only measured, or only predicted.
                None if config.separation.r2prime_strategy == R2PrimeStrategy::Auto && measured != predicted => {
                    if measured {
                        doc.say(&[Stage::R2], "R2' was computed as R2* - R2.".to_string());
                    } else {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., 2025{r2pn}).",
                        ));
                    }
                }
                Some(_) => doc.say(&[Stage::R2], 
                    "R2' maps were taken from the supplied BIDS derivatives.".to_string()),
                None => match config.separation.r2prime_strategy {
                    R2PrimeStrategy::Mese => doc.say(&[Stage::R2], 
                        "R2' was computed as R2* - R2.".to_string()),
                    R2PrimeStrategy::R2primenet => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], format!(
                            "R2' was estimated from the R2* map with R2PRIMEnet (Kim et al., \
                             2025{r2pn}).",
                        ));
                    }
                    R2PrimeStrategy::Auto => {
                        doc.cite(&CITE_R2PRIMENET);
                        doc.say(&[Stage::R2], if config.separation.r2primenet.ignore_field_mismatch {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and estimated from R2* with R2PRIMEnet \
                             (Kim et al., 2025{r2pn}) otherwise.")
                        } else {
                            format!("R2' was computed as R2* - R2 where a multi-echo spin-echo \
                             acquisition was available, and otherwise, at 3 T only, estimated \
                             from R2* with R2PRIMEnet (Kim et al., 2025{r2pn}).")
                        });
                    }
                },
            }
            // Both ways QSMxT computes an R2′ (R2* − R2 and R2PRIMEnet) set negative values to zero.
            if config.separation.custom_r2prime_tool.is_none() && !none_made {
                doc.say(&[Stage::R2], "Negative R2' values were set to zero.".to_string());
            }
            // Said where a run applied it off 3 T, or nothing is known of the runs.
            let off_field = ran.count(Topic::R2primenetCalibration, Choice::Uncalibrated) > 0
                || ran.0.is_none_or(|r| r.split(Topic::R2primeSource).is_empty());
            if config.separation.custom_r2prime_tool.is_none()
                && config.separation.r2prime_strategy != R2PrimeStrategy::Mese
                && config.separation.r2primenet.ignore_field_mismatch
                && off_field && !none_made
            {
                doc.say(&[Stage::R2], 
                    "R2PRIMEnet, whose weights were trained at 3 T, was also applied to data \
                     acquired at other field strengths."
                        .to_string());
            }
        }
    }

    // Chi-separation (susceptibility source separation)
    if config.pipeline.do_chi_separation {
        let (sep_name, sep_cite) = separation_name_cite(config.separation.algorithm);
        doc.cite(sep_cite);
        doc.say(&[Stage::ChiSeparation], format!(
            "Paramagnetic and diamagnetic susceptibility maps were computed using {} ({}) within {}{}.",
            sep_name, cite_inline(sep_cite),
            // Named in full by the referencing sentence, unless the map was left unreferenced.
            if config.pipeline.do_qsm && config.qsm.reference != QsmReference::None { RECON_MASK_AGAIN } else { RECON_MASK },
            separation_changes(config).comma_with(),
        ));
        // DECOMPOSE's signal phase comes from a QSM per echo: a per-echo QSM's, or the one QSM.
        if config.separation.algorithm == SeparationAlgorithm::Decompose {
            doc.say(&[Stage::ChiSeparation], if says_per_echo(config) && config.separation.custom_qsm_tool.is_none() {
                "Each echo's susceptibility map was used for the signal phase at that echo.".to_string()
            } else {
                "The same susceptibility map was used for the signal phase at every echo.".to_string()
            });
            if config.separation.decompose.unwrap_phase {
                doc.say(&[Stage::ChiSeparation], "The phase was unwrapped along the echoes.".to_string());
            }
            if config.separation.decompose.chi_bound == ChiBoundChoice::EchoSpacing {
                doc.say(&[Stage::ChiSeparation], chi_bound_sentence(ran));
            }
        }
        match config.separation.algorithm {
            SeparationAlgorithm::ChiSepIlsqr => doc.say(&[Stage::ChiSeparation],
                "The two sources were initialised from the susceptibility map and solved by LSQR, \
                 streaking removal and a non-negative refinement.".to_string()),
            SeparationAlgorithm::ChiSepMedi => {
                let r2star = match config.separation.chi_sep_medi.csf_r2star {
                    ChiSepCsfR2star::Arlo => {
                        doc.cite(&CITE_ARLO);
                        "ARLO R2* (Pei et al., 2015)"
                    }
                    ChiSepCsfR2star::Nnls => "R2* from a non-negative least-squares mono-exponential fit",
                };
                doc.say(&[Stage::ChiSeparation], format!(
                    "The two sources were initialised from the susceptibility map, the data terms were \
                     weighted by a noise map from the echo magnitudes, and the susceptibility within a \
                     CSF mask from {r2star} was regularised towards uniformity."));
            }
            _ => {}
        }
        // The networks' outputs as QSM.rs returns them.
        if matches!(config.separation.algorithm, SeparationAlgorithm::ChiSepNet | SeparationAlgorithm::SusepNet) {
            doc.say(&[Stage::ChiSeparation], "Negative χ+ and |χ−| values were set to zero.".to_string());
        }
        if crate::config::chisepnet_route_can_estimate_r2prime(config) {
            chisepnet_r2prime_sentences(&mut doc, config, ran);
        }
        if crate::stages::separation::reconstructs_qsm(config) {
            let mut q = Changed::default();
            let defaults = SeparationConfig::default();
            let d = match config.separation.algorithm {
                SeparationAlgorithm::ChiSepMedi => &defaults.chi_sep_medi.qsm_ilsqr,
                _ => &defaults.chi_sep_ilsqr.qsm_ilsqr,
            };
            q.add_fields(crate::stages::separation::initial_qsm_ilsqr(config), d, &[]);
            doc.cite(&CITE_ILSQR);
            doc.say(&[Stage::ChiSeparation], format!(
                "The susceptibility map separated was reconstructed from the local field by iLSQR ({}){}.",
                cite_inline(&CITE_ILSQR), q.with()));
        }
    }

    // Multi-orientation. Described after the per-orientation pipeline because that is the
    // order it runs in: each orientation is unwrapped and background-corrected on its own,
    // and only the local fields are combined. Whether a group forms depends on the runs, so the
    // setting alone says nothing happened: the record does ([`Topic::OrientationGroup`]). Before
    // any run, or where only some runs were combined, the wording is conditional (#496).
    let combined = ran.count(Topic::OrientationGroup, Choice::Applied);
    let every_run = combined > 0 && combined == ran.runs();
    if config.multi_orientation.enabled() && (ran.0.is_none() || combined > 0) {
        let fields = if every_run {
            "Local field maps from multiple head orientations"
        } else {
            "Where a session held several head orientations of the same object, local field maps \
             from those orientations"
        };
        match config.multi_orientation.algorithm {
            MultiOrientAlgorithm::Cosmos => {
                doc.cite(&CITE_COSMOS);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by Calculation Of Susceptibility through Multiple \
                     Orientation Sampling (COSMOS; Liu et al., 2009), using each orientation's B0 \
                     direction expressed in that common frame."),
                );
            }
            MultiOrientAlgorithm::Sti => {
                doc.cite(&CITE_STI);
                doc.say(&[], format!(
                    "{fields}, co-registered onto a common \
                     grid, were combined by susceptibility tensor imaging (STI; Liu, 2010), \
                     yielding the six independent components of the symmetric susceptibility \
                     tensor along with mean magnetic susceptibility (MMS), magnetic susceptibility \
                     anisotropy (MSA) and the principal eigenvector."),
                );
            }
        }
        // How the orientations got onto one grid is part of the method, not an implementation
        // detail: it determines where the B0 directions came from, which is half the input.
        if config.multi_orientation.register {
            let reference = config.multi_orientation.reference.trim();
            let target = if reference.is_empty() {
                "the first orientation's grid".to_string()
            } else {
                format!("the grid of the reference orientation ({reference})")
            };
            doc.say(&[], format!(
                "Orientations that were not already on a common grid were rigidly co-registered \
                 (6 degrees of freedom, normalized cross-correlation over a multi-resolution \
                 pyramid{}) onto {target} using the magnitude images, and each \
                 orientation's B0 direction was derived from the recovered rotation.",
                registration_changes(config).semi(),
            ));
        } else {
            doc.say(&[], 
                "The orientations were taken to be already co-registered onto a common grid."
                    .to_string(),
            );
        }
        if differs(config.multi_orientation.lambda, MultiOrientationConfig::default().lambda) {
            doc.say(&[], format!(
                "The multi-orientation inversion was regularized with lambda = {}.",
                config.multi_orientation.lambda
            ));
        }
        let m = &config.multi_orientation;
        if m.algorithm == MultiOrientAlgorithm::Cosmos {
            if m.magnitude_weighting {
                let d = MultiOrientationConfig::default();
                let mut cg = Changed::default();
                cg.add(differs(m.cosmos_tol, d.cosmos_tol), || format!("tolerance {}", m.cosmos_tol));
                cg.add(m.cosmos_max_iter != d.cosmos_max_iter, || format!("at most {} iterations", m.cosmos_max_iter));
                doc.say(&[], format!(
                    "Each orientation's field was weighted by its magnitude, and the weighted \
                     least-squares problem solved by conjugate gradient{}.", cg.paren()));
            }
            doc.say(&[], 
                "The COSMOS map was referenced in the same way as the single-orientation maps, \
                 over the voxels covered by every orientation's background-field-removal mask."
                    .to_string(),
            );
        }
    }

    // DICOM export. It is the front end's, after the run, and can fail without failing the run, so
    // the setting alone says nothing happened: the record does ([`RunRecord::dicom_exported`]).
    // Before any run the setting is described, as a native run exports; a browser run never does
    // (the browser profile clears the setting).
    let exported = ran.dicom_exported();
    if config.pipeline.export_dicom && (ran.0.is_none() || exported > 0) {
        doc.say(&[], if ran.0.is_none() || exported == ran.runs() {
            "Final maps were additionally exported as DICOM series.".to_string()
        } else {
            "For some of the runs, final maps were additionally exported as DICOM series.".to_string()
        });
    }

    doc
}
