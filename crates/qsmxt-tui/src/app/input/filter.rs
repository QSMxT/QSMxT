//! The BIDS run tree: which runs are processed, their orientation groups, and its keys.

use super::*;

// ─── Filter tree state ───

/// What is focused in the flattened filter tree view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterFocus {
    Include,
    Exclude,
    /// The orientation-group pattern: which runs are the same object at a different angle.
    OrientationGroup,
    TreeNode(usize), // index into the flattened visible node list
    NumEchoes,
}


/// Where ←/→ can put a run of the tree ([`FilterTreeState::orientation_slots`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationSlot {
    /// In no orientation group.
    None,
    /// In group `n` (from 0), not its reference.
    Member(usize),
    /// In group `n`, its reference.
    Reference(usize),
    /// In a group of its own, to put others with.
    New,
}

/// A run's subject and session, which an orientation group stays within.
fn session_of(key: &qsmxt_io::bids::entities::AcquisitionKey) -> String {
    format!("{}/{}", key.subject, key.session.as_deref().unwrap_or(""))
}

/// What an orientation-group pattern currently matches.
#[derive(Debug, Clone, Default)]
pub struct OrientationPreview {
    /// The pattern itself was rejected — shown instead of any groups.
    pub error: Option<String>,
    /// One entry per group: its label and its members' display names.
    pub groups: Vec<(String, Vec<String>)>,
    /// Selected runs the pattern did not put in any group. These process normally.
    pub ungrouped: usize,
    /// Each grouped run, by its key: its group's number (from 1, in group order) and whether it
    /// is the group's reference. Shown against the run in the tree.
    pub member_of: std::collections::HashMap<String, (usize, bool)>,
    /// The selected runs' keys, in discovery order.
    pub order: Vec<String>,
}

/// Which text field is being edited in the filter area.
enum FilterTextField {
    Include,
    Exclude,
    OrientationGroup,
}

/// A single visible row in the flattened tree (for navigation/rendering).
#[derive(Debug, Clone)]
pub enum TreeRow {
    Subject(usize),                 // index into tree.subjects
    Session(usize, usize),         // (subject_idx, session_idx)
    Run { sub: usize, ses: Option<usize>, run: usize }, // leaf run
}

/// Filter tab state: tree of BIDS runs with selection and navigation.
#[derive(Debug, Clone)]
pub struct FilterTreeState {
    pub tree: Option<BidsTree>,
    pub collapsed: HashSet<String>,
    pub focus: FilterFocus,
    pub include_pattern: String,
    pub include_editing: bool,
    pub include_cursor: usize,
    pub exclude_pattern: String,
    pub exclude_editing: bool,
    pub exclude_cursor: usize,
    pub num_echoes: String,
    pub num_echoes_editing: bool,
    pub num_echoes_cursor: usize,
    /// Orientation-group pattern; empty = multi-orientation off.
    pub orientation_group: String,
    pub orientation_editing: bool,
    pub orientation_cursor: usize,
    pub scanned_bids_dir: Option<String>,
    pub scroll_offset: usize,
    /// Set when the user manually toggles tree checkboxes (pattern may not match)
    pub manual_override: bool,
}

impl Default for FilterTreeState {
    fn default() -> Self {
        Self {
            tree: None,
            collapsed: HashSet::new(),
            focus: FilterFocus::Include,
            include_pattern: "*".to_string(),
            include_editing: false,
            include_cursor: 0,
            exclude_pattern: String::new(),
            exclude_editing: false,
            exclude_cursor: 0,
            num_echoes: String::new(),
            num_echoes_editing: false,
            num_echoes_cursor: 0,
            // Off by default: most datasets are single-orientation, and grouping runs that are
            // merely repeats would quietly turn a repeat into a bogus COSMOS set.
            orientation_group: String::new(),
            orientation_editing: false,
            orientation_cursor: 0,
            scanned_bids_dir: None,
            scroll_offset: 0,
            manual_override: false,
        }
    }
}

impl FilterTreeState {
    /// Build the flattened list of visible tree rows (respecting collapsed state).
    pub fn visible_rows(&self) -> Vec<TreeRow> {
        let Some(ref tree) = self.tree else { return Vec::new() };
        let mut rows = Vec::new();
        for (si, sub) in tree.subjects.iter().enumerate() {
            rows.push(TreeRow::Subject(si));
            if self.collapsed.contains(&format!("sub-{}", sub.name)) {
                continue;
            }
            // Direct runs (no session)
            for ri in 0..sub.runs.len() {
                rows.push(TreeRow::Run { sub: si, ses: None, run: ri });
            }
            // Sessions
            for (sei, ses) in sub.sessions.iter().enumerate() {
                rows.push(TreeRow::Session(si, sei));
                if self.collapsed.contains(&format!("sub-{}/ses-{}", sub.name, ses.name)) {
                    continue;
                }
                for ri in 0..ses.runs.len() {
                    rows.push(TreeRow::Run { sub: si, ses: Some(sei), run: ri });
                }
            }
        }
        rows
    }

    /// Total number of focusable items: pattern + tree rows + num_echoes.
    /// Move focus down.
    pub fn focus_next(&mut self) {
        match self.focus {
            FilterFocus::Include => {
                self.focus = FilterFocus::Exclude;
            }
            FilterFocus::Exclude => {
                self.focus = FilterFocus::OrientationGroup;
            }
            FilterFocus::OrientationGroup => self.focus = self.first_tree_focus(),
            FilterFocus::TreeNode(i) => {
                let rows = self.visible_rows().len();
                if i + 1 < rows {
                    self.focus = FilterFocus::TreeNode(i + 1);
                } else {
                    self.focus = FilterFocus::NumEchoes;
                }
            }
            FilterFocus::NumEchoes => {} // already at bottom
        }
    }

    /// Move focus up.
    pub fn focus_prev(&mut self) {
        match self.focus {
            FilterFocus::Include => {} // already at top
            FilterFocus::Exclude => self.focus = FilterFocus::Include,
            FilterFocus::OrientationGroup => self.focus = FilterFocus::Exclude,
            FilterFocus::TreeNode(0) => self.focus = FilterFocus::OrientationGroup,
            FilterFocus::TreeNode(i) => self.focus = FilterFocus::TreeNode(i - 1),
            FilterFocus::NumEchoes => {
                let rows = self.visible_rows().len();
                if rows > 0 {
                    self.focus = FilterFocus::TreeNode(rows - 1);
                } else {
                    self.focus = FilterFocus::OrientationGroup;
                }
            }
        }
    }

    /// Whether the orientation-group pattern is set to anything active.
    pub fn multi_orientation_on(&self) -> bool {
        let p = self.orientation_group.trim();
        !p.is_empty() && !p.eq_ignore_ascii_case("none")
    }

    fn first_tree_focus(&self) -> FilterFocus {
        if self.visible_rows().is_empty() {
            FilterFocus::NumEchoes
        } else {
            FilterFocus::TreeNode(0)
        }
    }

    /// What the current orientation-group pattern matches, recomputed on every keystroke.
    ///
    /// This is the part that makes the pattern usable. Nobody gets a grouping pattern right
    /// first try, and the cost of getting it wrong is not an error — it is a reconstruction
    /// that combines the wrong runs, or silently combines none and falls back to ordinary
    /// single-orientation processing. Showing the groups as they are typed turns that into
    /// something you can see.
    ///
    /// Only structure is previewed here, not physics: B0 directions live in sidecars and
    /// affines that would have to be read from disk on every keystroke. `qsmxt run --dry`
    /// prints the direction table and the degeneracy verdict for each group.
    ///
    /// `reference` is the orientation the others are registered onto (a setting of the config,
    /// `multi_orientation.reference`).
    pub fn orientation_preview(&self, reference: &str) -> OrientationPreview {
        if !self.multi_orientation_on() {
            return OrientationPreview::default();
        }
        let pattern = match qsmxt_io::bids::orientation::parse_pattern(&self.orientation_group) {
            Ok(Some(p)) => p,
            Ok(None) => return OrientationPreview::default(),
            Err(e) => return OrientationPreview { error: Some(e), ..Default::default() },
        };
        let Some(ref tree) = self.tree else { return OrientationPreview::default() };

        // Only selected runs can form a group — an orientation excluded by the filters above
        // is not going to be reconstructed, so counting it here would be a lie.
        let mut keys = Vec::new();
        let mut displays = Vec::new();
        tree.for_each_run(|leaf| {
            if leaf.selected {
                keys.push((leaf.key.clone(), leaf.key_string.clone()));
                displays.push(leaf.display.clone());
            }
        });

        let mut groups = qsmxt_io::bids::orientation::group_keys(&keys, &pattern);
        let grouped: usize = groups.iter().map(|g| g.members.len()).sum();
        // The reference is listed first, as the run will use it; one that does not resolve is
        // shown as the error the run would refuse with.
        let strings: Vec<String> = keys.iter().map(|(_, s)| s.clone()).collect();
        let error = qsmxt_io::bids::orientation::apply_reference(
            &mut groups, &strings, reference).err();
        let resolved = error.is_none();
        let member_of = groups.iter().enumerate()
            .flat_map(|(g, group)| group.members.iter().enumerate()
                .map(move |(j, &i)| (i, (g + 1, j == 0 && resolved))))
            .map(|(i, tag)| (strings[i].clone(), tag))
            .collect();
        let error = error.or_else(|| {
            let run_keys: Vec<_> = keys.iter().map(|(k, _)| k.clone()).collect();
            qsmxt_io::bids::orientation::distinct_outputs(&groups, &run_keys).err()
        });
        OrientationPreview {
            member_of,
            order: strings.clone(),
            error,
            groups: groups
                .into_iter()
                .map(|g| {
                    let mut members: Vec<String> = g.members.iter().map(|&i| displays[i].clone()).collect();
                    if let Some(first) = members.first_mut() {
                        first.push_str(" [reference]");
                    }
                    (g.label, members)
                })
                .collect(),
            ungrouped: keys.len().saturating_sub(grouped),
        }
    }

    /// The orientation grouping, trimmed, while one is set.
    pub fn orientation_group_by(&self) -> Option<String> {
        self.multi_orientation_on().then(|| self.orientation_group.trim().to_string())
    }

    /// Scan BIDS directory if it changed since last scan.
    pub fn maybe_rescan(&mut self, bids_dir: &str) {
        let trimmed = bids_dir.trim();
        let dir = if let Some(rest) = trimmed.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                format!("{}/{}", home.to_string_lossy(), rest)
            } else {
                trimmed.to_string()
            }
        } else {
            trimmed.to_string()
        };
        if dir.is_empty() {
            return;
        }
        if self.scanned_bids_dir.as_deref() == Some(&dir) {
            return;
        }
        match discovery::scan_bids_tree(Path::new(&dir)) {
            Ok(tree) => {
                self.tree = Some(tree);
                self.focus = FilterFocus::Include;
                self.scroll_offset = 0;
                self.apply_include_exclude();
            }
            Err(_) => {
                self.tree = None;
            }
        }
        self.scanned_bids_dir = Some(dir);
    }

    /// Apply include/exclude patterns to update tree checkbox state.
    pub fn apply_include_exclude(&mut self) {
        use qsmxt_io::bids::discovery::passes_include_exclude;
        let Some(ref mut tree) = self.tree else { return };

        let include = if self.include_pattern.trim().is_empty() {
            None
        } else {
            Some(self.include_pattern.split_whitespace().map(String::from).collect::<Vec<_>>())
        };
        let exclude = if self.exclude_pattern.trim().is_empty() {
            None
        } else {
            Some(self.exclude_pattern.split_whitespace().map(String::from).collect::<Vec<_>>())
        };

        tree.for_each_run_mut(|run| {
            run.selected = passes_include_exclude(&run.key_string, &include, &exclude);
        });
        self.manual_override = false;
    }

    /// Toggle the focused tree node (run leaf or subject/session toggle).
    pub fn toggle_focused(&mut self) {
        let rows = self.visible_rows();
        let FilterFocus::TreeNode(idx) = self.focus else { return };
        let Some(row) = rows.get(idx) else { return };
        let Some(ref mut tree) = self.tree else { return };

        match row {
            TreeRow::Subject(si) => {
                let sub = &tree.subjects[*si];
                let new_val = sub.selected_runs() < sub.total_runs();
                tree.subjects[*si].set_all(new_val);
            }
            TreeRow::Session(si, sei) => {
                let ses = &tree.subjects[*si].sessions[*sei];
                let new_val = ses.runs.iter().any(|r| !r.selected);
                tree.subjects[*si].sessions[*sei].set_all(new_val);
            }
            TreeRow::Run { sub, ses, run } => {
                match ses {
                    Some(sei) => {
                        let r = &mut tree.subjects[*sub].sessions[*sei].runs[*run];
                        r.selected = !r.selected;
                    }
                    None => {
                        let r = &mut tree.subjects[*sub].runs[*run];
                        r.selected = !r.selected;
                    }
                }
            }
        }
    }

    /// The focused run of the tree, if a run is focused.
    fn focused_run(&mut self) -> Option<&mut qsmxt_io::bids::discovery::BidsRunLeaf> {
        let FilterFocus::TreeNode(idx) = self.focus else { return None };
        let row = self.visible_rows().get(idx).cloned()?;
        let tree = self.tree.as_mut()?;
        match row {
            TreeRow::Run { sub, ses: Some(sei), run } => Some(&mut tree.subjects[sub].sessions[sei].runs[run]),
            TreeRow::Run { sub, ses: None, run } => Some(&mut tree.subjects[sub].runs[run]),
            _ => None,
        }
    }

    /// The orientation groups as run keys, each with its reference first: as listed (`runs:`),
    /// or as a typed pattern groups the selected runs now.
    pub fn orientation_groups(&self, reference: &str) -> Vec<Vec<String>> {
        if let Some(groups) = qsmxt_io::bids::orientation::listed_groups(&self.orientation_group) {
            return groups;
        }
        let preview = self.orientation_preview(reference);
        let mut groups: Vec<Vec<String>> = vec![Vec::new(); preview.groups.len()];
        let mut members: Vec<(&String, &(usize, bool))> = preview.member_of.iter().collect();
        members.sort_by_key(|(k, (g, reference))| (*g, !*reference, preview.order.iter().position(|o| o == *k)));
        for (key, (g, _)) in members {
            groups[g - 1].push(key.clone());
        }
        groups
    }

    /// Where ←/→ can put the focused run, in order, and where it is now: no group, then each
    /// group of its session as a member and as the reference, then a new group.
    pub fn orientation_slots(&mut self, reference: &str) -> Option<(Vec<OrientationSlot>, usize)> {
        let groups = self.orientation_groups(reference);
        let run = self.focused_run()?;
        let (key, session) = (run.key_string.clone(), session_of(&run.key));
        let keys = self.run_keys();
        let same_session = |k: &String| keys.get(k).is_some_and(|s| *s == session);
        let mut slots = vec![OrientationSlot::None];
        let mut now = 0;
        for (g, members) in groups.iter().enumerate() {
            let others: Vec<&String> = members.iter().filter(|k| **k != key).collect();
            if others.is_empty() || !others.iter().all(|k| same_session(k)) {
                continue;
            }
            for slot in [OrientationSlot::Member(g), OrientationSlot::Reference(g)] {
                let here = match slot {
                    OrientationSlot::Member(_) => members.iter().skip(1).any(|k| *k == key),
                    _ => members.first() == Some(&key),
                };
                if here {
                    now = slots.len();
                }
                slots.push(slot);
            }
        }
        if groups.iter().any(|g| g.as_slice() == [key.clone()]) {
            now = slots.len();
        }
        slots.push(OrientationSlot::New);
        Some((slots, now))
    }

    /// ←/→ on a run: move it to the slot before or after its own ([`Self::orientation_slots`]).
    /// The groups are kept as the pattern itself, listed (`runs:`), each with its reference
    /// first, so the config and the command line need nothing new. A typed pattern's groups
    /// are where this starts from.
    ///
    /// Each group's reference is then its first run: the caller clears the reference setting.
    pub fn move_orientation(&mut self, forward: bool, reference: &str) {
        let Some((slots, now)) = self.orientation_slots(reference) else { return };
        let next = if forward { (now + 1).min(slots.len() - 1) } else { now.saturating_sub(1) };
        if next == now {
            return;
        }
        let mut groups = self.orientation_groups(reference);
        let Some(run) = self.focused_run() else { return };
        let key = run.key_string.clone();
        if slots[next] != OrientationSlot::None {
            // Only a selected run is processed, so grouping one selects it.
            run.selected = true;
        }
        for g in &mut groups {
            g.retain(|k| *k != key);
        }
        match slots[next] {
            OrientationSlot::None => {}
            OrientationSlot::Member(g) => groups[g].push(key),
            OrientationSlot::Reference(g) => groups[g].insert(0, key),
            OrientationSlot::New => groups.push(vec![key]),
        }
        self.orientation_group = qsmxt_io::bids::orientation::runs_pattern(&groups);
        self.orientation_cursor = self.orientation_group.len();
    }

    /// Every run's key, with its subject and session.
    fn run_keys(&self) -> std::collections::HashMap<String, String> {
        let mut out = std::collections::HashMap::new();
        if let Some(ref tree) = self.tree {
            tree.for_each_run(|leaf| {
                out.insert(leaf.key_string.clone(), session_of(&leaf.key));
            });
        }
        out
    }

    /// Toggle collapse on the focused subject or session.
    pub fn toggle_collapse(&mut self) {
        let rows = self.visible_rows();
        let FilterFocus::TreeNode(idx) = self.focus else { return };
        let Some(row) = rows.get(idx) else { return };
        let Some(ref tree) = self.tree else { return };

        let key = match row {
            TreeRow::Subject(si) => format!("sub-{}", tree.subjects[*si].name),
            TreeRow::Session(si, sei) => format!("sub-{}/ses-{}", tree.subjects[*si].name, tree.subjects[*si].sessions[*sei].name),
            _ => return,
        };
        if self.collapsed.contains(&key) {
            self.collapsed.remove(&key);
        } else {
            self.collapsed.insert(key);
        }
    }

    /// Get include/exclude filter args for command building.
    /// When manual_override is set, returns exact key list as include patterns.
    pub fn get_include_exclude(&self) -> (Option<Vec<String>>, Option<Vec<String>>) {
        if self.manual_override {
            // Manual toggle: emit whichever of --include or --exclude is shorter
            let Some(ref tree) = self.tree else { return (None, None) };
            let total = tree.total_runs();
            let selected = tree.selected_runs();
            if selected == total {
                return (None, None); // all selected, no filter needed
            }
            let mut included = Vec::new();
            let mut excluded = Vec::new();
            tree.for_each_run(|run| {
                if run.selected {
                    included.push(run.key_string.clone());
                } else {
                    excluded.push(run.key_string.clone());
                }
            });
            if included.is_empty() {
                return (None, None);
            }
            // Pick the shorter representation
            if excluded.len() <= included.len() {
                excluded.sort();
                (None, Some(excluded))
            } else {
                included.sort();
                (Some(included), None)
            }
        } else {
            // Pattern mode: pass through include/exclude patterns
            let include = if self.include_pattern.trim().is_empty() || self.include_pattern.trim() == "*" {
                None
            } else {
                Some(self.include_pattern.split_whitespace().map(String::from).collect())
            };
            let exclude = if self.exclude_pattern.trim().is_empty() {
                None
            } else {
                Some(self.exclude_pattern.split_whitespace().map(String::from).collect())
            };
            (include, exclude)
        }
    }
}

impl App {
    pub(super) fn handle_filter_key(&mut self, key: KeyEvent) {
        // Handle editing mode (include, exclude, or num_echoes text input)
        if self.filter_state.include_editing {
            self.handle_filter_text_key(key, FilterTextField::Include);
            return;
        }
        if self.filter_state.exclude_editing {
            self.handle_filter_text_key(key, FilterTextField::Exclude);
            return;
        }
        if self.filter_state.orientation_editing {
            self.handle_filter_text_key(key, FilterTextField::OrientationGroup);
            return;
        }
        if self.filter_state.num_echoes_editing {
            self.handle_filter_num_echoes_key(key);
            return;
        }

        // Navigation mode
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,

            // Tab switching (same as other tabs)
            // 1..=N, Tab, Shift-Tab: one rule for every tab (see `switch_tab`).
            _ if self.switch_tab(key) => {}

            // Navigation within filter tree
            KeyCode::Up | KeyCode::Char('k') => self.filter_state.focus_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.filter_state.focus_next(),

            // A run: its orientation group. A subject or session: collapse/expand.
            KeyCode::Left | KeyCode::Right if self.filter_state.focused_run().is_some() => {
                let reference = self.field_text("multi_orientation.reference").into_owned();
                self.filter_state.move_orientation(key.code == KeyCode::Right, &reference);
                // Each group's reference is now its first run.
                self.form.settings.reset("multi_orientation.reference", false);
            }
            KeyCode::Left | KeyCode::Right => self.filter_state.toggle_collapse(),

            // Toggle / interact
            KeyCode::Char(' ') => {
                self.filter_state.toggle_focused();
                self.filter_state.manual_override = true;
            }
            KeyCode::Enter => {
                match self.filter_state.focus {
                    FilterFocus::Include => {
                        self.filter_state.include_editing = true;
                        self.filter_state.include_cursor = self.filter_state.include_pattern.len();
                    }
                    FilterFocus::Exclude => {
                        self.filter_state.exclude_editing = true;
                        self.filter_state.exclude_cursor = self.filter_state.exclude_pattern.len();
                    }
                    FilterFocus::TreeNode(_) => {
                        self.filter_state.toggle_focused();
                        self.filter_state.manual_override = true;
                    }
                    FilterFocus::OrientationGroup => {
                        self.filter_state.orientation_editing = true;
                        self.filter_state.orientation_cursor = self.filter_state.orientation_group.len();
                    }
                    FilterFocus::NumEchoes => {
                        self.filter_state.num_echoes_editing = true;
                        self.filter_state.num_echoes_cursor = self.filter_state.num_echoes.len();
                    }
                }
            }

            // Select all / none
            KeyCode::Char('a') => {
                if let Some(ref mut tree) = self.filter_state.tree {
                    tree.set_all(true);
                }
                self.filter_state.manual_override = true;
            }
            KeyCode::Char('n') => {
                if let Some(ref mut tree) = self.filter_state.tree {
                    tree.set_all(false);
                }
                self.filter_state.manual_override = true;
            }

            KeyCode::F(5) => self.try_run(),

            _ => {}
        }
    }

    fn handle_filter_text_key(&mut self, key: KeyEvent, field: FilterTextField) {
        let (text, cursor, editing) = match field {
            FilterTextField::Include => (
                &mut self.filter_state.include_pattern,
                &mut self.filter_state.include_cursor,
                &mut self.filter_state.include_editing,
            ),
            FilterTextField::Exclude => (
                &mut self.filter_state.exclude_pattern,
                &mut self.filter_state.exclude_cursor,
                &mut self.filter_state.exclude_editing,
            ),
            FilterTextField::OrientationGroup => (
                &mut self.filter_state.orientation_group,
                &mut self.filter_state.orientation_cursor,
                &mut self.filter_state.orientation_editing,
            ),
        };
        match key.code {
            KeyCode::Esc => {
                *editing = false;
            }
            KeyCode::Enter => {
                *editing = false;
                self.filter_state.apply_include_exclude();
            }
            KeyCode::Char(c) => {
                text.insert(*cursor, c);
                *cursor += 1;
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && *cursor > 0 => {
                let new_pos = word_boundary_left(text, *cursor);
                text.drain(new_pos..*cursor);
                *cursor = new_pos;
            }
            KeyCode::Backspace if *cursor > 0 => {
                *cursor -= 1;
                text.remove(*cursor);
            }
            KeyCode::Delete if *cursor < text.len() => {
                text.remove(*cursor);
            }
            KeyCode::Left => *cursor = cursor.saturating_sub(1),
            KeyCode::Right if *cursor < text.len() => {
                *cursor += 1;
            }
            KeyCode::Home => *cursor = 0,
            KeyCode::End => *cursor = text.len(),
            _ => {}
        }
    }

    fn handle_filter_num_echoes_key(&mut self, key: KeyEvent) {
        let fs = &mut self.filter_state;
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                fs.num_echoes_editing = false;
            }
            KeyCode::Char(c) => {
                fs.num_echoes.insert(fs.num_echoes_cursor, c);
                fs.num_echoes_cursor += 1;
            }
            KeyCode::Backspace if key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) && fs.num_echoes_cursor > 0 => {
                let new_pos = word_boundary_left(&fs.num_echoes, fs.num_echoes_cursor);
                fs.num_echoes.drain(new_pos..fs.num_echoes_cursor);
                fs.num_echoes_cursor = new_pos;
            }
            KeyCode::Backspace if fs.num_echoes_cursor > 0 => {
                fs.num_echoes_cursor -= 1;
                fs.num_echoes.remove(fs.num_echoes_cursor);
            }
            KeyCode::Delete
                if fs.num_echoes_cursor < fs.num_echoes.len() => {
                    fs.num_echoes.remove(fs.num_echoes_cursor);
                }
            KeyCode::Left => fs.num_echoes_cursor = fs.num_echoes_cursor.saturating_sub(1),
            KeyCode::Right
                if fs.num_echoes_cursor < fs.num_echoes.len() => {
                    fs.num_echoes_cursor += 1;
                }
            KeyCode::Home => fs.num_echoes_cursor = 0,
            KeyCode::End => fs.num_echoes_cursor = fs.num_echoes.len(),
            _ => {}
        }
    }
}
