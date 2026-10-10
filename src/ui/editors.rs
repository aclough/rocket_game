//! The engine, reactor and power editors: lockstep scale/enrichment
//! edits applied to a project, and their key handling.

use super::*;
use super::designer::sync_stages_to_projects;

/// What the Cycle row steps through: every cycle the player can use,
/// plus "Solid motor" as its own entry. A solid is a pressure-fed cycle
/// on the solid preset, which nobody looking for a solid thinks to try;
/// listing it beside the cycles is what makes it findable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CycleChoice {
    Cycle(EngineCycle),
    SolidMotor,
}

impl CycleChoice {
    /// The choice a project's (cycle, preset) shows on the row.
    pub(super) fn of(cycle: EngineCycle, preset: PropellantPreset) -> Self {
        if preset == PropellantPreset::Solid { CycleChoice::SolidMotor } else { CycleChoice::Cycle(cycle) }
    }

    /// The (cycle, preset) a choice sets, keeping `preset` where the
    /// cycle still takes it.
    fn family(self, preset: PropellantPreset) -> (EngineCycle, PropellantPreset) {
        match self {
            CycleChoice::SolidMotor => (EngineCycle::PressureFed, PropellantPreset::Solid),
            CycleChoice::Cycle(cycle) => {
                let keep = preset != PropellantPreset::Solid && preset.compatible_cycles().contains(&cycle);
                let preset = if keep { preset } else {
                    PropellantPreset::ALL.iter().copied()
                        .find(|p| *p != PropellantPreset::Solid && p.compatible_cycles().contains(&cycle))
                        .unwrap_or(preset)
                };
                (cycle, preset)
            }
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            CycleChoice::SolidMotor => "Solid motor",
            CycleChoice::Cycle(cycle) => cycle.display_name(),
        }
    }
}

/// The Cycle row's entries, in order, for the player's unlocked tech.
pub(super) fn available_cycle_choices(game: &GameState) -> Vec<CycleChoice> {
    let mut choices = vec![
        CycleChoice::Cycle(EngineCycle::PressureFed),
        CycleChoice::SolidMotor,
        CycleChoice::Cycle(EngineCycle::GasGenerator),
        CycleChoice::Cycle(EngineCycle::Expander),
        CycleChoice::Cycle(EngineCycle::StagedCombustion),
        CycleChoice::Cycle(EngineCycle::FullFlow),
    ];
    if game.tech_unlocked(crate::technology::TECH_NUCLEAR_THERMAL) {
        choices.push(CycleChoice::Cycle(EngineCycle::NuclearThermal));
    }
    choices.push(CycleChoice::Cycle(EngineCycle::ElectricPropulsion));
    choices.push(CycleChoice::Cycle(EngineCycle::SolarSail));
    choices
}

/// How many SI units one typed unit of an engine's size headline is:
/// the size field is typed in kN (thrust), kW (power) or m² (sail).
fn headline_unit_si(unit: crate::engine_project::SizeUnit) -> f64 {
    match unit {
        crate::engine_project::SizeUnit::ThroatArea | crate::engine_project::SizeUnit::InputPower => 1_000.0,
        crate::engine_project::SizeUnit::SailArea => 1.0,
    }
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
            ep.scale(&self.game.balance.nozzle),
        ))
    }

    /// The engine's size headline (thrust, input power or sail area, in
    /// SI units) and the unit it is measured in.
    fn engine_headline(&self, project_id: crate::engine_project::EngineProjectId)
        -> Option<(f64, crate::engine_project::SizeUnit)>
    {
        let ep = self.game.player_company.find_engine_project(project_id)?;
        Some((crate::engine_project::headline(&ep.design),
              crate::engine_project::SizeUnit::of_cycle(ep.design.cycle)))
    }

    /// Resize the engine so its headline figure is `target` (SI units),
    /// held inside its kind's range.
    pub(super) fn apply_engine_headline(&mut self, project_id: crate::engine_project::EngineProjectId, target: f64) {
        let Some((_, cycle, preset, _)) = self.editor_snapshot(project_id) else { return };
        let (lo, hi) = crate::engine_project::SizeUnit::of_cycle(cycle).headline_range();
        if let Some(scale) = crate::engine_project::scale_for_headline(
            cycle, preset, target.clamp(lo, hi), &self.game.balance.nozzle,
        ) {
            self.apply_engine_scale(project_id, scale);
        }
    }

    /// Change an engine's family. Within one propulsion kind the engine
    /// keeps its headline figure (a 900 kN kerolox engine becomes a
    /// 900 kN methalox one); across kinds the numbers mean different
    /// things, so it takes the new family's default size.
    fn apply_engine_family(
        &mut self, project_id: crate::engine_project::EngineProjectId,
        name: String, cycle: EngineCycle, preset: PropellantPreset,
    ) {
        use crate::engine_project::{scale_for_headline, SizeUnit, DEFAULT_SCALE};
        let Some((old_headline, old_unit)) = self.engine_headline(project_id) else { return };
        let scale = if SizeUnit::of_cycle(cycle) == old_unit {
            scale_for_headline(cycle, preset, old_headline, &self.game.balance.nozzle).unwrap_or(DEFAULT_SCALE)
        } else {
            DEFAULT_SCALE
        };
        if let Some(ep) = self.game.player_company.find_engine_project_mut(project_id) {
            ep.apply_edit(name, cycle, preset, scale, &self.game.balance);
        }
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
    /// project; a parseable size (in kN, kW or m²) is clamped to the
    /// kind's range and applied. Either way the designer's stages (if the editor was
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
            EditorField::Size | EditorField::Scale => match (buffer.parse::<f64>(), self.engine_headline(project_id)) {
                (Ok(parsed), Some((_, unit))) => {
                    self.apply_engine_headline(project_id, parsed * headline_unit_si(unit));
                    true
                }
                _ => false,
            },
        };
        if changed {
            if let Some(s) = state.as_mut() {
                sync_stages_to_projects(s, &self.game.player_company, &self.game.balance.nozzle);
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
            // A reactor is sized by scale; `Size` is the engine field.
            EditorField::Size | EditorField::Scale => {
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
        let (_, scale, enrichment, is_proposed) = snap;

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
                // The field opens blank: typing a new name should not
                // start with deleting the old one. Enter on an empty
                // field keeps the current name.
                self.input_mode = InputMode::ReactorEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: String::new(),
                };
            }
            KeyCode::Down => {
                cursor_down(&mut cursor, ROW_COUNT);
                self.input_mode = InputMode::ReactorEditor { project_id, cursor };
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
    /// 2=Preset, 3=Size. Left/Right cycles values on Cycle/Preset and
    /// steps Size by ×√2 within its kind's range; Enter on Name/Size
    /// opens a text/number sub-modal (Size typed in kN, kW or m²).
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
        let (name, cycle, preset, _) = snap;
        let row_count = 4; // Name, Cycle, Preset, Size
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
                // The field opens blank: typing a new name should not
                // start with deleting the old one. Enter on an empty
                // field keeps the current name.
                self.input_mode = InputMode::EngineEditorField {
                    project_id, cursor, field: EditorField::Name, buffer: String::new(), state,
                };
            }
            KeyCode::Down => {
                cursor_down(&mut cursor, row_count);
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Enter if cursor == 3 => {
                let buffer = self.engine_headline(project_id)
                    .map_or(String::new(), |(value, unit)| format!("{:.0}", value / headline_unit_si(unit)));
                self.input_mode = InputMode::EngineEditorField {
                    project_id, cursor, field: EditorField::Size, buffer, state,
                };
            }
            KeyCode::Left | KeyCode::Right if cursor == 1 => {
                let choices = available_cycle_choices(&self.game);
                let current = CycleChoice::of(cycle, preset);
                let next = wrap_cycle(&choices, current, matches!(key, KeyCode::Right))
                    .unwrap_or(current);
                let (next_cycle, new_preset) = next.family(preset);
                self.apply_engine_family(project_id, name, next_cycle, new_preset);
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company, &self.game.balance.nozzle);
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
                self.apply_engine_family(project_id, name, cycle, next);
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company, &self.game.balance.nozzle);
                }
                self.input_mode = InputMode::EngineEditor { project_id, cursor, state };
            }
            KeyCode::Left | KeyCode::Right if cursor == 3 => {
                if let Some((value, unit)) = self.engine_headline(project_id) {
                    let (lo, hi) = unit.headline_range();
                    let target = stepped_scale(value, matches!(key, KeyCode::Right), lo, hi);
                    self.apply_engine_headline(project_id, target);
                }
                if let Some(s) = state.as_mut() {
                    sync_stages_to_projects(s, &self.game.player_company, &self.game.balance.nozzle);
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
    use crate::engine_project::{headline, EngineProject, EngineProjectId};

    fn app_with_engine() -> (App, EngineProjectId) {
        let mut game = crate::game_state::GameState::new("Field Test".into(), 5);
        let ep = EngineProject::new(
            EngineProjectId(1), EngineId(1), "Family".into(),
            EngineCycle::GasGenerator, PropellantPreset::Kerolox, 1.0,
            &game.balance,
        ).unwrap();
        let pid = ep.project_id;
        game.player_company.engine_projects.push(ep);
        (App::new(game), pid)
    }

    /// The engine's size headline: thrust for a bell (N).
    fn thrust_of(app: &App, pid: EngineProjectId) -> f64 {
        headline(&app.game.player_company.find_engine_project(pid).unwrap().design)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-6 * b.abs()
    }

    /// Enter on the size row opens the field with the thrust in kN;
    /// typing a value and Enter applies it clamped to the kind's range
    /// and returns to the same row.
    #[test]
    fn typing_a_size_applies_it_clamped_and_returns_to_the_row() {
        let (mut app, pid) = app_with_engine();
        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 3, state: None };
        app.handle_key(KeyCode::Enter);
        match &app.input_mode {
            InputMode::EngineEditorField { field: EditorField::Size, buffer, .. } => {
                assert_eq!(buffer, "900", "the kerolox gas generator's default, in kN");
            }
            other => panic!("expected the size field, got {other:?}"),
        }
        for k in [KeyCode::Backspace, KeyCode::Backspace, KeyCode::Backspace,
                  KeyCode::Char('2'), KeyCode::Char('x'), KeyCode::Char('5'), KeyCode::Char('0')] {
            app.handle_key(k);
        }
        app.handle_key(KeyCode::Enter);
        assert!(matches!(app.input_mode, InputMode::EngineEditor { cursor: 3, .. }));
        assert!(close(thrust_of(&app, pid), 250_000.0), "250 kN (the x is filtered)");
        // Past the top of the range it clamps to 10 MN.
        app.handle_key(KeyCode::Enter);
        for _ in 0..3 { app.handle_key(KeyCode::Backspace); }
        for c in "99999".chars() { app.handle_key(KeyCode::Char(c)); }
        app.handle_key(KeyCode::Enter);
        assert!(close(thrust_of(&app, pid), 10_000_000.0), "clamps to the maximum");
    }

    /// Changing propellant keeps the thrust the player chose; changing
    /// to another propulsion kind (electric) takes its default size,
    /// since kN and kW are not the same number.
    #[test]
    fn a_family_change_keeps_the_thrust_within_a_kind() {
        let (mut app, pid) = app_with_engine();
        app.apply_engine_headline(pid, 400_000.0);
        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 2, state: None };
        app.handle_key(KeyCode::Right); // Kerolox → the next preset for a gas generator
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        assert_ne!(ep.spec.preset, PropellantPreset::Kerolox);
        assert!(close(headline(&ep.design), 400_000.0), "thrust kept across propellants");
        // Size follows the throat, not the thrust: a different propellant
        // or cycle needs a different throat for the same thrust.
        assert!(ep.spec.size > 0.0);

        app.apply_engine_family(pid, "Ion".into(), EngineCycle::ElectricPropulsion, PropellantPreset::Xenon);
        let ep = app.game.player_company.find_engine_project(pid).unwrap();
        assert!(close(ep.design.power_draw_w(), 30_000.0), "electric takes its 30 kW default");
        assert!(close(ep.spec.size, 30_000.0), "an electric thruster is sized by its power");
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
        assert!(close(thrust_of(&app, pid), 900_000.0), "Esc keeps the default 900 kN");

        app.input_mode = InputMode::EngineEditor { project_id: pid, cursor: 0, state: None };
        app.handle_key(KeyCode::Enter);
        assert!(matches!(&app.input_mode,
            InputMode::EngineEditorField { field: EditorField::Name, buffer, .. } if buffer.is_empty()),
            "the name field opens blank");
        app.handle_key(KeyCode::Char(' '));
        app.handle_key(KeyCode::Enter);
        assert_eq!(app.game.player_company.find_engine_project(pid).unwrap().design.name, "Family");
    }
}
