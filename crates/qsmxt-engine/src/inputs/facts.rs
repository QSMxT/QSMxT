//! [`ScanInputs::facts`]: a scan's facts from its inputs, and what is wrong with them.

use super::*;

impl ScanInputs {
    /// The scan's facts, where each came from, and what is wrong with the inputs.
    ///
    /// - Every echo is given the same way ([`Encoding`]): each needs its phase image, its real and
    ///   imaginary parts, or each coil's phase and magnitude images, with as many coils as the
    ///   first echo. A magnitude image beside real and imaginary parts or coils is not used.
    /// - Matrix size and voxel size are the first echo's first image's (its phase image, real
    ///   part or first coil's phase image). Every other image must have the same matrix size (an
    ///   error otherwise); one placed or oriented differently with the same size is a warning,
    ///   since its voxels are then combined as if on one grid.
    /// - Magnitude is used when every echo has one; some but not all is an error. Real and
    ///   imaginary parts, and coils, give it.
    /// - Echo times are the typed ones, else each sidecar's `EchoTime`. Field strength is the
    ///   typed one, else the sidecars' `MagneticFieldStrength` (the last echo that gives one, as
    ///   `qsmxt` reads a BIDS run). `B0_dir` likewise; without one, a run takes B0 from the
    ///   image's affine (or resamples, as the run's settings decide). The manufacturer is the
    ///   typed one, else the first sidecar's that gives one; it decides the sign of the phase
    ///   computed from real and imaginary parts, and every sidecar a run reads carries it.
    /// - A MESE's echoes must share its first echo's grid; their echo times are the typed ones,
    ///   else each echo's sidecar's.
    pub fn facts(&self) -> ScanReport {
        let mut problems = Vec::new();
        let n = self.echoes.len();
        let encoding = self.encoding();
        let n_coils = self.n_coils();
        let slots = self.slots();
        if n == 0 {
            problems.push(InputProblem::error("No phase images: add each echo's phase image")
                .fact(FactName::Echoes).part(Part::Phase));
        }

        // Every echo given the same way, with what that way needs.
        let first_way = self.echoes.iter().position(|e| !e.encodings().is_empty()).map_or(1, |i| i + 1);
        for (i, echo) in self.echoes.iter().enumerate() {
            let e = i + 1;
            let ways = echo.encodings();
            if let [a, b, ..] = ways[..] {
                problems.push(InputProblem::error(format!(
                    "Echo {e} is given both as {} and as {}: give each echo one way", a.describe(), b.describe())).echo(e));
            } else if let [way] = ways[..] {
                if way != encoding {
                    problems.push(InputProblem::error(format!(
                        "Echo {e} is given as {}, but echo {first_way} as {}: give every echo of a scan the same way",
                        way.describe(), encoding.describe())).echo(e));
                }
            }
            if echo.magnitude.is_some() && encoding != Encoding::MagnitudePhase {
                problems.push(InputProblem::warning(format!(
                    "Echo {e}'s magnitude image is not used: the magnitude comes from the {}", encoding.describe()))
                    .echo(e).part(Part::Magnitude));
            }
            if encoding == Encoding::Coils && echo.coils.len() != n_coils {
                let problem = if echo.coils.is_empty() {
                    InputProblem::error(format!("Echo {e} has no coil images")).part(Part::Phase)
                } else {
                    InputProblem::error(format!(
                        "Echo {e} has {} coils but echo 1 has {n_coils}: every echo needs the same coils",
                        echo.coils.len()))
                };
                problems.push(problem.fact(FactName::Coils).echo(e));
            }
        }
        for slot in slots.iter().filter(|s| s.required && !s.part.is_mese() && self.geometry(s).is_none()) {
            let why = match (slot.coil, slot.part) {
                (Some(_), Part::Magnitude) => ": coil combination needs every coil's magnitude",
                _ => "",
            };
            let echo = format!("echo {}", slot.echo);
            let what = slot.name().replacen(&format!("{echo}'s "), "", 1);
            problems.push(InputProblem::error(format!("{} has no {what}{why}", capitalised(echo))).at(slot));
        }

        // Every image on the first echo's first image's grid.
        let reference = slots.iter().find(|s| s.echo == 1 && s.required && !s.part.is_mese())
            .and_then(|s| self.geometry(s).map(|g| (s, g)));
        let first = reference.map(|(_, g)| g);
        if let Some((reference_slot, reference)) = reference {
            for slot in slots.iter().filter(|s| !s.part.is_mese() && *s != reference_slot) {
                let Some(grid) = self.geometry(slot) else { continue };
                if grid.dims != reference.dims {
                    problems.push(InputProblem::error(format!(
                        "{} is {} but {} is {}: every image of a scan must come from the same acquisition",
                        capitalised(slot.name()), size(grid), reference_slot.name(), size(reference)))
                        .fact(FactName::Dims).at(slot));
                } else if !crate::stages::r2::same_grid(grid.dims, &grid.affine, reference.dims, &reference.affine) {
                    problems.push(InputProblem::warning(format!(
                        "{} has the same matrix size as {} but a different position, orientation or \
                         voxel size: they are combined as if on one grid",
                        capitalised(slot.name()), reference_slot.name())).fact(FactName::Dims).at(slot));
                }
            }
        }

        // Magnitude: all echoes, or none (real and imaginary parts and coils give it).
        let has_magnitude = match encoding {
            Encoding::MagnitudePhase => {
                let with_magnitude = self.echoes.iter().filter(|e| e.magnitude.is_some()).count();
                if with_magnitude > 0 && with_magnitude < n {
                    for (e, echo) in self.echoes.iter().enumerate() {
                        if echo.magnitude.is_none() {
                            problems.push(InputProblem::error(format!(
                                "Echo {} has no magnitude image: give every echo's magnitude, or none", e + 1))
                                .fact(FactName::Magnitude).echo(e + 1).part(Part::Magnitude));
                        }
                    }
                }
                n > 0 && with_magnitude == n
            }
            Encoding::RealImaginary | Encoding::Coils => n > 0,
        };
        if encoding == Encoding::Coils {
            if let Some(why) = crate::requirements::coil_combination_problem(n, n_coils) {
                problems.push(InputProblem::error(why).fact(FactName::Coils));
            }
        }

        // Echo times.
        let mut echo_times = Vec::with_capacity(n);
        let te_source = if let Some(tes) = self.overrides.echo_times.as_ref() {
            if tes.len() != n {
                problems.push(InputProblem::error(format!(
                    "{} echo times were typed for {n} echo{}", tes.len(), if n == 1 { "" } else { "es" }))
                    .fact(FactName::EchoTimes));
            } else if let Some(bad) = tes.iter().position(|t| !(t.is_finite() && *t > 0.0)) {
                problems.push(InputProblem::error(format!("Echo {}'s echo time must be a positive number", bad + 1))
                    .fact(FactName::EchoTimes).echo(bad + 1));
            } else {
                echo_times = tes.clone();
            }
            Source::Typed
        } else {
            for (e, echo) in self.echoes.iter().enumerate() {
                match number(echo.sidecar.as_ref(), "EchoTime") {
                    Some(te) => echo_times.push(te),
                    None => problems.push(InputProblem::error(format!(
                        "Echo {} has no echo time: add its JSON sidecar, or type the echo times", e + 1))
                        .fact(FactName::EchoTimes).echo(e + 1).part(Part::Sidecar)),
                }
            }
            if echo_times.len() == n && n > 0 { Source::Sidecar } else { Source::Missing }
        };
        let echo_times_known = echo_times.len() == n && n > 0;

        // Field strength and B0 direction: typed, else the last sidecar that gives one.
        let sidecars = || self.echoes.iter().filter_map(|e| e.sidecar.as_ref());
        let (field_strength, field_source) = match self.overrides.field_strength {
            Some(t) => (Some(t), Source::Typed),
            None => match sidecars().rev().find_map(|s| number(Some(s), "MagneticFieldStrength")) {
                Some(t) => (Some(t), Source::Sidecar),
                None => (None, Source::Missing),
            },
        };
        match field_strength {
            None if n > 0 => problems.push(InputProblem::error("No field strength: add the JSON sidecars, or type it")
                .fact(FactName::FieldStrength).part(Part::Sidecar)),
            Some(t) if !(t.is_finite() && t > 0.0) => problems.push(InputProblem::error(
                "The field strength must be a positive number of tesla").fact(FactName::FieldStrength)),
            _ => {}
        }
        let mut b0_dir = None;
        let mut b0_source = Source::Missing;
        if let Some(dir) = self.overrides.b0_dir {
            b0_dir = Some(dir);
            b0_source = Source::Typed;
        } else {
            for (e, echo) in self.echoes.iter().enumerate() {
                let Some(value) = echo.sidecar.as_ref().and_then(|s| s.get("B0_dir")) else { continue };
                let parts: Vec<f64> = value.as_array().map(|a| a.iter().filter_map(|x| x.as_f64()).collect())
                    .unwrap_or_default();
                if let [x, y, z] = parts[..] {
                    b0_dir = Some((x, y, z));
                    b0_source = Source::Sidecar;
                } else {
                    problems.push(InputProblem::warning(format!(
                        "Echo {}'s sidecar gives a B0 direction with {} components (three are needed): \
                         it is ignored", e + 1, parts.len())).fact(FactName::B0Dir).echo(e + 1).part(Part::Sidecar));
                }
            }
        }

        // The manufacturer: typed, else the first sidecar that gives one.
        let typed_manufacturer = self.overrides.manufacturer.as_deref().map(str::trim).filter(|m| !m.is_empty());
        let (manufacturer, manufacturer_source) = match typed_manufacturer {
            Some(m) => (Some(m.to_string()), Source::Typed),
            None => match sidecars().find_map(|s| s.get("Manufacturer")?.as_str()) {
                Some(m) => (Some(m.to_string()), Source::Sidecar),
                None => (None, Source::Missing),
            },
        };
        let negates = crate::stages::phase::negates_real_imaginary_phase(manufacturer.as_deref());
        let manufacturer_note = (encoding == Encoding::RealImaginary).then(|| if negates {
            "The phase from the real and imaginary parts is negated: this manufacturer records it with the opposite sign"
        } else {
            "Decides the sign of the phase from the real and imaginary parts"
        }.to_string());
        if encoding == Encoding::RealImaginary && manufacturer.is_none() && n > 0 {
            problems.push(InputProblem::warning(
                "The scanner's manufacturer is not known: the phase from the real and imaginary parts keeps \
                 its sign, which is right unless the scanner is a GE one. Add the JSON sidecars, or type the \
                 manufacturer").fact(FactName::Manufacturer).part(Part::Sidecar));
        }

        // The MESE: its echoes on its first echo's grid, and their echo times.
        let mese = self.mese.as_ref().filter(|m| !m.echoes.is_empty());
        let n_mese = mese.map_or(0, |m| m.echoes.len());
        let (mut mese_echo_times, mut mese_source) = (None, Source::Missing);
        if let Some(mese) = mese {
            let before = problems.len();
            let mese_slots: Vec<&Slot> = slots.iter().filter(|s| s.part == Part::MeseMagnitude).collect();
            let reference = mese_slots.first().and_then(|s| self.geometry(s));
            for slot in &mese_slots {
                match (self.geometry(slot), reference) {
                    (None, _) => problems.push(InputProblem::error(format!("MESE echo {} has no image", slot.echo))
                        .fact(FactName::MeseEchoes).at(slot)),
                    (Some(_), _) if slot.echo == 1 => {}
                    (Some(grid), Some(reference)) if grid.dims != reference.dims => problems.push(InputProblem::error(format!(
                        "MESE echo {}'s image is {} but MESE echo 1's is {}: every echo of the MESE must come \
                         from the same acquisition", slot.echo, size(grid), size(reference)))
                        .fact(FactName::MeseEchoes).at(slot)),
                    (Some(grid), Some(reference))
                        if !crate::stages::r2::same_grid(grid.dims, &grid.affine, reference.dims, &reference.affine) =>
                        problems.push(InputProblem::error(format!(
                            "MESE echo {}'s image has the same matrix size as MESE echo 1's but a different \
                             position, orientation or voxel size: every echo of the MESE must come from the \
                             same acquisition", slot.echo)).fact(FactName::MeseEchoes).at(slot)),
                    _ => {}
                }
            }
            let mut tes = Vec::with_capacity(n_mese);
            if let Some(typed) = self.overrides.mese_echo_times.as_ref() {
                mese_source = Source::Typed;
                if typed.len() != n_mese {
                    problems.push(InputProblem::error(format!(
                        "{} MESE echo times were typed for {n_mese} MESE echo{}", typed.len(),
                        if n_mese == 1 { "" } else { "es" })).fact(FactName::MeseEchoTimes));
                } else if let Some(bad) = typed.iter().position(|t| !(t.is_finite() && *t > 0.0)) {
                    problems.push(InputProblem::error(format!("MESE echo {}'s echo time must be a positive number", bad + 1))
                        .fact(FactName::MeseEchoTimes).echo(bad + 1).part(Part::MeseMagnitude));
                } else {
                    tes = typed.clone();
                }
            } else {
                mese_source = Source::Sidecar;
                for (i, echo) in mese.echoes.iter().enumerate() {
                    match number(echo.sidecar.as_ref(), "EchoTime") {
                        Some(te) => tes.push(te),
                        None => problems.push(InputProblem::error(format!(
                            "MESE echo {} has no echo time: add its JSON sidecar, or type the MESE's echo times", i + 1))
                            .fact(FactName::MeseEchoTimes).echo(i + 1).part(Part::MeseSidecar)),
                    }
                }
            }
            if tes.len() == n_mese {
                mese_echo_times = Some(tes);
            } else {
                mese_source = Source::Missing;
            }
            // A MESE with a problem is not one the run can measure R2 from.
            if problems[before..].iter().any(|p| p.severity == Severity::Error) {
                mese_echo_times = None;
            }
        } else if self.overrides.mese_echo_times.is_some() {
            problems.push(InputProblem::warning("MESE echo times were typed but no MESE images were given: they are not used")
                .fact(FactName::MeseEchoTimes));
        }
        let mese_times_shown = mese.and(mese_echo_times.clone()).or_else(|| {
            // Shown as far as they are known, even when the MESE has another problem.
            let mese = mese?;
            let tes = match self.overrides.mese_echo_times.as_ref() {
                Some(t) => t.clone(),
                None => mese.echoes.iter().map(|e| number(e.sidecar.as_ref(), "EchoTime")).collect::<Option<Vec<_>>>()?,
            };
            (tes.len() == n_mese).then_some(tes)
        });

        let fact = |name: FactName, value: serde_json::Value, source: Source, note: Option<String>| Fact {
            name, label: name.label(), units: name.units(), value, source, typed: name.typed(), note,
        };
        let grid_value = |f: &dyn Fn(&Geometry) -> serde_json::Value| first.map_or(serde_json::Value::Null, f);
        let header = |known: bool| if known { Source::Header } else { Source::Missing };
        let null = || serde_json::Value::Null;
        let shown = vec![
            fact(FactName::Dims, grid_value(&|g| serde_json::json!([g.dims.0, g.dims.1, g.dims.2])),
                header(first.is_some()), None),
            fact(FactName::VoxelSize, grid_value(&|g| serde_json::json!([g.voxel_size.0, g.voxel_size.1, g.voxel_size.2])),
                header(first.is_some()), None),
            fact(FactName::Echoes, n.into(), Source::Files, None),
            fact(FactName::EchoTimes, if echo_times_known { echo_times.clone().into() } else { null() },
                if echo_times_known { te_source } else { Source::Missing }, None),
            fact(FactName::FieldStrength, field_strength.map_or(null(), Into::into), field_source, None),
            fact(FactName::B0Dir, b0_dir.map_or(null(), |(x, y, z)| serde_json::json!([x, y, z])),
                b0_source, b0_dir.is_none().then(|| {
                    "From the image's orientation, or resampled to axial where the chosen methods need it".to_string()
                })),
            fact(FactName::Magnitude, has_magnitude.into(), Source::Files, match encoding {
                Encoding::RealImaginary => Some("From the real and imaginary parts".into()),
                Encoding::Coils if n_coils > 1 => Some("Combined from the coils".into()),
                _ => None,
            }),
            fact(FactName::Manufacturer, manufacturer.clone().map_or(null(), Into::into), manufacturer_source,
                manufacturer_note),
            fact(FactName::RealImaginary, (encoding == Encoding::RealImaginary).into(), Source::Files, None),
            fact(FactName::Coils, n_coils.into(), Source::Files,
                (encoding == Encoding::Coils && n_coils == 1).then(|| "One coil: used as combined data".to_string())),
            fact(FactName::MeseEchoes, n_mese.into(), Source::Files, None),
            fact(FactName::MeseEchoTimes, mese_times_shown.clone().map_or(null(), Into::into),
                if mese_times_shown.is_some() { mese_source } else { Source::Missing }, None),
        ];

        // Only the images the encoding reads: a magnitude image beside real and imaginary parts is not kept.
        let files = slots.iter().filter_map(|s| {
            self.geometry(s)?;
            Some(InputFile { key: s.item.key(), echo: s.echo, coil: s.coil, part: s.part, own_grid: s.part.is_mese() })
        }).collect();

        let usable = n > 0 && echo_times_known && first.is_some()
            && field_strength.is_some_and(|t| t.is_finite() && t > 0.0);
        let (facts, sidecars) = match (usable, first, field_strength) {
            (true, Some(grid), Some(field_strength)) => {
                // The centre frequency, as for the field strength: the last sidecar that gives one.
                let imaging_frequency = (field_source != Source::Typed)
                    .then(|| sidecars().rev().find_map(|s| number(Some(s), "ImagingFrequency")))
                    .flatten()
                    .filter(|f| f.is_finite() && *f > 0.0);
                let sidecars = self.echoes.iter().zip(&echo_times).map(|(echo, &te)| {
                    let mut sc = echo.sidecar.as_ref()
                        .and_then(|v| {
                            let mut v = v.clone();
                            let obj = v.as_object_mut()?;
                            obj.insert("EchoTime".into(), te.into());
                            obj.insert("MagneticFieldStrength".into(), field_strength.into());
                            QsmSidecar::from_json(&v).ok()
                        })
                        .unwrap_or_else(|| QsmSidecar::bare(te, field_strength));
                    sc.b0_dir = b0_dir.map(|(x, y, z)| vec![x, y, z]);
                    sc.manufacturer = manufacturer.clone();
                    // A typed field strength is what the user wants phase converted with.
                    if field_source == Source::Typed {
                        sc.imaging_frequency = None;
                    }
                    sc
                }).collect();
                let facts = ScanFacts {
                    dims: grid.dims,
                    voxel_size: Some(grid.voxel_size),
                    n_echoes: n,
                    echo_times,
                    has_magnitude,
                    n_coils,
                    real_imaginary: encoding == Encoding::RealImaginary,
                    field_strength: crate::scan::working_field_strength(field_strength, imaging_frequency),
                    nominal_field_strength: imaging_frequency.map(|_| field_strength),
                    imaging_frequency,
                    b0_dir,
                    mese_echo_times,
                    custom_mask: false,
                    custom_dseg: false,
                    custom_qsm: false,
                    custom_r2: false,
                    custom_r2prime: false,
                    manufacturer: manufacturer.clone(),
                };
                (Some(facts), sidecars)
            }
            _ => (None, Vec::new()),
        };
        ScanReport { facts, shown, problems, sidecars, files }
    }
}
