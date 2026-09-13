//! The engine, reactor and power editors: lockstep scale/enrichment
//! edits applied to a project, and their key handling.

use super::*;
use super::designer::sync_stages_to_projects;

/// Engine cycles available to the player based on unlocked tech.
fn available_engine_cycles(game: &GameState) -> Vec<EngineCycle> {
    let mut cycles = vec![
        EngineCycle::PressureFed,
        EngineCycle::GasGenerator,
        EngineCycle::Expander,
        EngineCycle::StagedCombustion,
        EngineCycle::FullFlow,
    ];
    if game.technologies.iter().any(|t|
        t.id == crate::technology::TECH_NUCLEAR_THERMAL && t.unlocked
    ) {
        cycles.push(EngineCycle::NuclearThermal);
    }
    cycles.push(EngineCycle::ElectricPropulsion);
    cycles.push(EngineCycle::SolarSail);
    cycles
}

/// One arrow-key step of an editor's scale: ×√2 up or ÷√2 down, held
/// inside the project kind's `[min, max]`.
fn stepped_scale(scale: f64, up: bool, min: f64, max: f64) -> f64 {
    if up {
        (scale * std::f64::consts::SQRT_2).min(max)
    } else {
        (scale / std::f64::consts::SQRT_2).max(min)
    }
}

impl App {
    /// Snapshot of an engine project for editor display + mutation.
    /// The nozzle variant is deliberately absent — it is chosen per
    /// stage in the rocket designer, not on the engine project.
    pub(super) fn editor_snapshot(&self, project_id: crate::engine_project::EngineProjectId)
        -> Option<(String, EngineCycle, PropellantPreset, f64)>
    {
        let ep = self.game.player_company.find_engine_project(project_id)?;
        Some((
            ep.design.name.clone(),
            ep.design.cycle,
            ep.spec.preset,
            ep.spec.scale,
        ))
    }

    /// Apply an arbitrary scale to the engine project, rebuilding its
    /// design through `apply_edit`.
    pub(super) fn apply_engine_scale(&mut self, project_id: crate::engine_project::EngineProjectId, scale: f64) {
        let snap = match self.editor_snapshot(project_id) {
            Some(s) => s, None => return,
        };
        let (name, cycle, preset, _) = snap;
        if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
            ep.apply_edit(name, cycle, preset, scale, &self.game.balance);
        }
    }

    /// Apply a new `scale` to a reactor project, preserving its name
    /// and enrichment. Snapshots the design first so we can rebuild
    /// without taking two overlapping borrows on the project.
    pub(super) fn apply_reactor_scale(
        &mut self,
        project_id: crate::reactor_project::ReactorProjectId,
        scale: f64,
    ) {
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.enrichment),
            None => return,
        };
        let (name, enrichment) = snap;
        if let Some(rp) = self.game.player_company.find_reactor_project_mut(project_id) {
            rp.apply_edit(name, scale, enrichment, &self.game.balance);
        }
    }

    /// Enter on an engine editor field: a non-empty name renames the
    /// project; a parseable scale is clamped to the engine range and
    /// applied. Either way the designer's stages (if the editor was
    /// opened from it) follow the project.
    pub(super) fn commit_engine_field(
        &mut self,
        project_id: crate::engine_project::EngineProjectId,
        field: EditorField,
        buffer: &str,
        state: &mut Option<Box<RocketDesignerState>>,
    ) {
        let changed = match field {
            EditorField::Name => {
                let new_name = buffer.trim().to_string();
                if new_name.is_empty() { return; }
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.design.name = new_name;
                }
                true
            }
            EditorField::Scale => match buffer.parse::<f64>() {
                Ok(parsed) => {
                    let clamped = parsed.clamp(
                        crate::engine_project::MIN_SCALE, crate::engine_project::MAX_SCALE);
                    self.apply_engine_scale(project_id, clamped);
                    true
                }
                Err(_) => false,
            },
        };
        if changed {
            if let Some(s) = state.as_mut() {
                sync_stages_to_projects(s, &self.game.player_company);
            }
        }
    }

    /// Enter on a reactor editor field; the reactor twin of
    /// `commit_engine_field`, with no designer to keep in step.
    pub(super) fn commit_reactor_field(
        &mut self,
        project_id: crate::reactor_project::ReactorProjectId,
        field: EditorField,
        buffer: &str,
    ) {
        match field {
            EditorField::Name => {
                let new_name = buffer.trim().to_string();
                if new_name.is_empty() { return; }
                if let Some(rp) = self.game.player_company.find_reactor_project_mut(project_id) {
                    rp.design.name = new_name;
                }
            }
            EditorField::Scale => {
                if let Ok(parsed) = buffer.parse::<f64>() {
                    let clamped = parsed.clamp(
                        crate::reactor::MIN_SCALE, crate::reactor::MAX_SCALE);
                    self.apply_reactor_scale(project_id, clamped);
                }
            }
        }
    }

    /// Apply a new enrichment to a reactor project, preserving its
    /// name and scale.
    pub(super) fn apply_reactor_enrichment(
        &mut self,
        project_id: crate::reactor_project::ReactorProjectId,
        enrichment: crate::reactor::EnrichmentLevel,
    ) {
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.scale),
            None => return,
        };
        let (name, scale) = snap;
        if let Some(rp) = self.game.player_company.find_reactor_project_mut(project_id) {
            rp.apply_edit(name, scale, enrichment, &self.game.balance);
        }
    }

    /// Reactor editor key handler. Cursor: 0 = Name, 1 = Scale,
    /// 2 = Enrichment. Left/Right adjusts the scalar on Scale (×√2)
    /// or cycles through reputation-unlocked enrichments. Enter opens
    /// a sub-modal on Name/Scale. D promotes a Proposed reactor to
    /// InDesign and closes the modal; Esc closes — deleting the
    /// project if it was still Proposed.
    pub(super) fn handle_reactor_editor_key(
        &mut self,
        key: KeyCode,
        project_id: crate::reactor_project::ReactorProjectId,
        mut cursor: usize,
    ) {
        use crate::reactor::available_enrichments;
        use crate::reactor_project::ReactorDesignStatus;
        const ROW_COUNT: usize = 3; // Name, Scale, Enrichment

        // If the project disappeared underneath us, bail cleanly.
        let snap = match self.game.player_company.find_reactor_project(project_id) {
            Some(rp) => (rp.design.name.clone(), rp.design.scale, rp.design.enrichment,
                         matches!(rp.status, ReactorDesignStatus::Proposed { .. })),
            None => {
                self.exit_modal();
                return;
            }
        };
        let (name, scale, enrichment, is_proposed) = snap;

        if cursor >= ROW_COUNT { cursor = ROW_COUNT - 1; }

        match key {
            KeyCode::Esc => {
                // Cancel: drop a draft we created; leave real work
                // alone. Proposed reactors only ever exist for the
                // lifetime of the editor session that birthed them.
                if is_proposed {
                    self.game.player_company.delete_proposed(ProjectRef::Reactor(project_id));
                }
                self.exit_modal();
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                // Done: promote Proposed → InDesign and log the event,
                // then close. No-op for projects already past Proposed
                // (they only land here via the "edit existing" path).
                if let Some(rname) = self.game.player_company.promote_proposed(ProjectRef::Reactor(project_id)) {
                    self.game.log(crate::event::GameEvent::project(
                        crate::project::ProjectKind::Reactor, rname,
                        crate::event::ProjectEvent::DesignStarted,
                    ));
                }
                self.exit_modal();
            }
            KeyCode::Up => {
                cursor_up(&mut cursor);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::ReactorEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: name,
                };
            }
            KeyCode::Down => {
                cursor_down(&mut cursor, ROW_COUNT);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::ReactorEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: name,
                };
            }
            KeyCode::Enter if cursor == 1 => {
                self.input_mode = InputMode::ReactorEditorField {
                    project_id, cursor, field: EditorField::Scale, buffer: format!("{:.2}", scale),
                };
            }
            KeyCode::Left | KeyCode::Right if cursor == 1 => {
                let new_scale = stepped_scale(
                    scale, matches!(key, KeyCode::Right),
                    crate::reactor::MIN_SCALE, crate::reactor::MAX_SCALE,
                );
                self.apply_reactor_scale(project_id, new_scale);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            KeyCode::Left | KeyCode::Right if cursor == 2 => {
                // Cycle through reputation-unlocked enrichments. The
                // current enrichment is always considered "in the list"
                // even if reputation has since fallen below the gate,
                // so a player who built an HEU reactor doesn't get the
                // editor refusing to display HEU when re-opened later.
                let reputation = self.game.player_company.reputation.total();
                let mut levels = available_enrichments(reputation, &self.game.balance.reputation);
                if !levels.contains(&enrichment) {
                    levels.push(enrichment);
                    levels.sort_by_key(|e| *e as u32);
                }
                if levels.len() > 1 {
                    let next = wrap_cycle(&levels, enrichment, matches!(key, KeyCode::Right))
                        .unwrap_or(enrichment);
                    self.apply_reactor_enrichment(project_id, next);
                }
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
            _ => {
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
            }
        }
    }

    /// Engine editor key handler. Cursor walks: 0=Name, 1=Cycle,
    /// 2=Preset, 3=Scale. Left/Right cycles values on Cycle/Preset;
    /// +/- adjusts Scale by ×√2 (and clamps to [MIN_SCALE, MAX_SCALE]);
    /// Enter on Name/Scale opens a text/number sub-modal.
    ///
    /// There is no nozzle row: a project designs an engine *family*
    /// and each stage picks its own bell in the rocket designer.
    pub(super) fn handle_engine_editor_key(
        &mut self,
        key: KeyCode,
        project_id: crate::engine_project::EngineProjectId,
        mut cursor: usize,
        mut state: Option<Box<RocketDesignerState>>,
    ) {
        let snap = match self.editor_snapshot(project_id) {
            Some(s) => s,
            None => {
                // Project disappeared (shouldn't happen) — bail back to
                // wherever we came from.
                match state {
                    Some(s) => self.input_mode = InputMode::designer(s),
                    None => self.exit_modal(),
                }
                return;
            }
        };
        let (name, cycle, preset, scale) = snap;
        let row_count = 4; // Name, Cycle, Preset, Scale
        if cursor >= row_count { cursor = row_count - 1; }

        match key {
            KeyCode::Esc => {
                match state {
                    // From the rocket designer: return there (the engine
                    // stays as edited; it commits with the rocket).
                    Some(s) => self.input_mode = InputMode::designer(s),
                    // Standalone: cancel the draft we created.
                    None => {
                        self.game.player_company.delete_proposed(ProjectRef::Engine(project_id));
                        self.exit_modal();
                    }
                }
            }
            // Standalone only: commit the draft to InDesign.
            KeyCode::Char('d') | KeyCode::Char('D') if state.is_none() => {
                if let Some(name) = self.game.player_company.promote_proposed(ProjectRef::Engine(project_id)) {
                    self.game.log(crate::event::GameEvent::project(
                        crate::project::ProjectKind::Engine, name,
                        crate::event::ProjectEvent::DesignStarted,
                    ));
                }
                self.exit_modal();
            }
            KeyCode::Up => {
                cursor_up(&mut cursor);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::EngineEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: name, state,
                };
            }
            KeyCode::Down => {
                cursor_down(&mut cursor, row_count);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Enter if cursor == 0 => {
                self.input_mode = InputMode::EngineEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: name, state,
                };
            }
            KeyCode::Enter if cursor == 3 => {
                self.input_mode = InputMode::EngineEditorField {
                    project_id, cursor, field: EditorField::Scale,
                    buffer: format!("{:.2}", scale), state,
                };
            }
            KeyCode::Left | KeyCode::Right if cursor == 1 => {
                let cycles = available_engine_cycles(&self.game);
                let next = wrap_cycle(&cycles, cycle, matches!(key, KeyCode::Right))
                    .unwrap_or(cycle);
                // Keep current preset if it's still compatible with the
                // new cycle; otherwise pick the first compatible preset.
                let new_preset = if preset.compatible_cycles().contains(&next) {
                    preset
                } else {
                    PropellantPreset::ALL.iter()
                        .copied()
                        .find(|p| p.compatible_cycles().contains(&next))
                        .unwrap_or(preset)
                };
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.apply_edit(name, next, new_preset, scale, &self.game.balance);
                }
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company);
                }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left | KeyCode::Right if cursor == 2 => {
                let presets: Vec<PropellantPreset> = PropellantPreset::ALL.iter()
                    .filter(|p| p.compatible_cycles().contains(&cycle))
                    .copied()
                    .collect();
                let next = wrap_cycle(&presets, preset, matches!(key, KeyCode::Right))
                    .unwrap_or(preset);
                if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
                    ep.apply_edit(name, cycle, next, scale, &self.game.balance);
                }
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company);
                }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left | KeyCode::Right if cursor == 3 => {
                let new_scale = stepped_scale(
                    scale, matches!(key, KeyCode::Right),
                    crate::engine_project::MIN_SCALE, crate::engine_project::MAX_SCALE,
                );
                self.apply_engine_scale(project_id, new_scale);
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company);
                }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            _ => {
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
        }
    }

    /// Power-source editor key handler. The cursor walks a merged list:
    /// rows 0..N for currently-equipped sources, then rows N..N+P for
    /// the preset-add menu. Space adds a preset; X/Del removes an
    /// equipped source. Esc returns to the designer.
    pub(super) fn handle_power_editor_key(
        &mut self,
        key: KeyCode,
        mut state: Box<RocketDesignerState>,
        group_index: usize,
        stage_index: usize,
        mut cursor: usize,
    ) {
        // Sanity-bound the indices in case the design changed underneath
        // us (shouldn't, but be defensive).
        let stage = state.stage_groups
            .get(group_index)
            .and_then(|g| g.get(stage_index));
        if stage.is_none() {
            self.input_mode = InputMode::designer(state);
            return;
        }
        let n_equipped = state.stage_groups[group_index][stage_index]
            .power_sources.len();
        // Reactor designs the player has researched at least to
        // Testing. Snapshot the design now so the cursor-region math
        // and the install step agree (no second borrow on Company).
        let player_reactor_designs: Vec<crate::reactor::ReactorDesign> =
            self.game.player_company.installable_reactor_projects()
                .map(|rp| rp.design.clone())
                .collect();
        let n_reactors = player_reactor_designs.len();
        // Filter the preset catalog to only those whose tech is unlocked.
        let available_presets: Vec<&crate::power::PowerPreset> =
            crate::power::power_presets().iter()
                .filter(|p| crate::power::preset_available(p, &self.game.technologies))
                .collect();
        let n_total = n_equipped + n_reactors + available_presets.len();
        // Cursor regions:
        //   [0, n_equipped)                          equipped sources
        //   [n_equipped, n_equipped + n_reactors)    player reactors
        //   [..n_total)                              presets
        let reactor_start = n_equipped;
        let preset_start = n_equipped + n_reactors;

        match key {
            KeyCode::Esc => {
                self.input_mode = InputMode::designer(state);
                return;
            }
            KeyCode::Up => cursor_up(&mut cursor),
            KeyCode::Down => cursor_down(&mut cursor, n_total),
            KeyCode::Char(' ') => {
                if cursor >= preset_start {
                    let pi = cursor - preset_start;
                    let preset = available_presets[pi];
                    // Solar-panel preset: size to the current stage's
                    // demand instead of using the placeholder closure.
                    let new_src = if preset.auto_size_solar {
                        crate::power::solar_panel_for_stage_demand(
                            &state.stage_groups[group_index][stage_index],
                        )
                    } else {
                        (preset.build)(&self.game.balance.costs)
                    };
                    state.stage_groups[group_index][stage_index]
                        .power_sources.push(new_src);
                    cursor = n_equipped;
                } else if cursor >= reactor_start {
                    let ri = cursor - reactor_start;
                    let design = player_reactor_designs[ri].clone();
                    let new_src = crate::power::PowerSource::from_reactor_design(design);
                    state.stage_groups[group_index][stage_index]
                        .power_sources.push(new_src);
                    cursor = n_equipped;
                }
            }
            KeyCode::Char('x') | KeyCode::Char('X') | KeyCode::Delete => {
                if cursor < n_equipped {
                    state.stage_groups[group_index][stage_index]
                        .power_sources.remove(cursor);
                    let new_n_equipped = state.stage_groups[group_index][stage_index]
                        .power_sources.len();
                    if cursor >= new_n_equipped && cursor > 0 {
                        cursor -= 1;
                    }
                }
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Resize a solar panel up by √2 (two presses = 2×).
                if cursor < n_equipped {
                    let src = &mut state.stage_groups[group_index][stage_index]
                        .power_sources[cursor];
                    if let crate::power::PowerSourceKind::SolarPanel { peak_w_at_1au } = src.kind {
                        src.resize_solar_panel(peak_w_at_1au * std::f64::consts::SQRT_2);
                    }
                }
            }
            KeyCode::Char('-') | KeyCode::Char('_')
                // Resize a solar panel down by 1/√2 (symmetric with +).
                if cursor < n_equipped => {
                    let src = &mut state.stage_groups[group_index][stage_index]
                        .power_sources[cursor];
                    if let crate::power::PowerSourceKind::SolarPanel { peak_w_at_1au } = src.kind {
                        src.resize_solar_panel(
                            (peak_w_at_1au / std::f64::consts::SQRT_2).max(1.0),
                        );
                    }
                }
            _ => {}
        }
        self.input_mode = InputMode::designer_with(
            state, DesignerSubMode::PowerEditor { group_index, stage_index, cursor },
        );
    }
}

#[cfg(test)]
mod field_tests {
    use super::*;
    use crate::engine::EngineId;
    use crate::engine_project::{EngineProject, EngineProjectId, MAX_SCALE};

    fn app_with_engine() -> (App, EngineProjectId) {
        let mut game = crate::game_state::GameState::new(
            "Field Test".into(), 200_000_000.0, 5,
        );
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Family".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0,
            &game.balance,
        ).unwrap();
        let pid = ep.project_id;
        game.player_company.engine_projects.push(ep);
        (App::new(game), pid)
    }

    fn scale_of(app: &App, pid: EngineProjectId) -> f64 {
        app.game.player_company.find_engine_project(pid).unwrap().spec.scale
    }

    /// Enter on the scale row opens the field with the current scale;
    /// typing a value and Enter applies it clamped to the engine
    /// range and returns to the same row.
    #[test]
    fn typing_a_scale_applies_it_clamped_and_returns_to_the_row() {
        let (mut app, pid) = app_with_engine();
        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 3, state: None };
        app.handle_key(KeyCode::Enter);
        match &app.input_mode {
            InputMode::EngineEditorField { field: EditorField::Scale, buffer, .. } => {
                assert_eq!(buffer, "1.00");
            }
            other => panic!("expected the scale field, got {other:?}"),
        }
        for k in [KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace,
                  KeyCode::Char('9'), KeyCode::Char('x'), KeyCode::Char('9')] {
            app.handle_key(k);
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.input_mode, InputMode::EngineEditor { cursor: 3, .. }));
        assert_eq!(scale_of(&app, pid), MAX_SCALE, "99 (the x is filtered) clamps to the maximum");
    }

    /// Esc drops the typed value, and a renamed engine keeps its name
    /// only when the field is not blank.
    #[test]
    fn esc_discards_and_a_blank_name_is_ignored() {
        let (mut app, pid) = app_with_engine();
        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 3, state: None };
        app.handle_key(KeyCode::Enter);
        app.handle_key(KeyCode::Char('2'));
        app.handle_key(KeyCode::Esc);
        assert_eq!(scale_of(&app, pid), 1.0);

        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 0, state: None };
        app.handle_key(KeyCode::Enter);
        for _ in 0..10 { app.handle_key(KeyCode::Backspace); }
        app.handle_key(KeyCode::Char(' '));
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.game.player_company.find_engine_project(pid).unwrap().design.name, "Family");
    }
}
