//! R&D projects: starting engine, reactor and rocket projects, finding
//! them by id, and the kind-agnostic dispatch across the three lists.

use super::*;

impl Company {
    /// Start a new engine design project. Returns the event if successful.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn start_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        technology_id: Option<crate::technology::TechnologyId>,
        balance_cfg: &BalanceConfig,
    ) -> Option<GameEvent> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let mut project = EngineProject::new(
            project_id, engine_id, name.clone(),
            cycle, preset, scale, balance_cfg,
        )?;
        project.technology_id = technology_id;
        self.engine_projects.push(project);
        Some(GameEvent::project(crate::project::ProjectKind::Engine, name, ProjectEvent::DesignStarted))
    }

    /// Start a tentative engine design in `Proposed` status. Used by the
    /// rocket designer; the engine doesn't enter the regular project
    /// queue until the parent rocket is finalised. Returns the new
    /// project id on success.
    #[allow(clippy::too_many_arguments)] // constructor-style, callers read positionally with names at the call site
    pub fn start_proposed_engine_project(
        &mut self,
        name: String,
        cycle: EngineCycle,
        preset: PropellantPreset,
        scale: f64,
        technology_id: Option<crate::technology::TechnologyId>,
        balance_cfg: &BalanceConfig,
    ) -> Option<EngineProjectId> {
        let project_id = EngineProjectId(self.next_project_id);
        let engine_id = EngineId(self.next_engine_id);
        self.next_project_id += 1;
        self.next_engine_id += 1;

        let mut project = EngineProject::new_proposed(
            project_id, engine_id, name,
            cycle, preset, scale, balance_cfg,
        )?;
        project.technology_id = technology_id;
        self.engine_projects.push(project);
        Some(project_id)
    }

    /// Iterator over engine projects that should be visible in the
    /// engines pane — everything except `Proposed`, which belongs to an
    /// in-progress rocket designer session, and everything the player
    /// has retired.
    pub fn visible_engine_projects(&self) -> impl Iterator<Item = (usize, &EngineProject)> {
        self.engine_projects.iter()
            .enumerate()
            .filter(|(_, ep)| !matches!(ep.status, EngineDesignStatus::Proposed { .. }))
            .filter(|(_, ep)| !ep.retired)
    }

    /// Look up an engine project by id.
    pub fn find_engine_project(&self, id: EngineProjectId) -> Option<&EngineProject> {
        self.engine_projects.iter().find(|ep| ep.project_id == id)
    }

    /// Look up an engine project by id, mutably.
    pub fn find_engine_project_mut(&mut self, id: EngineProjectId) -> Option<&mut EngineProject> {
        self.engine_projects.iter_mut().find(|ep| ep.project_id == id)
    }

    /// Spawn a `Proposed` reactor project the editor can iterate on
    /// without committing the player to real work. Promoted to
    /// `InDesign` via `promote_proposed_reactor` when the player hits
    /// Done; deleted via `delete_proposed_reactor` on cancel.
    pub fn start_proposed_reactor(
        &mut self,
        name: String,
        scale: f64,
        enrichment: crate::reactor::EnrichmentLevel,
        balance_cfg: &BalanceConfig,
    ) -> crate::reactor_project::ReactorProjectId {
        let project_id = crate::reactor_project::ReactorProjectId(self.next_reactor_project_id);
        let reactor_id = crate::reactor::ReactorId(self.next_reactor_id);
        self.next_reactor_project_id += 1;
        self.next_reactor_id += 1;
        let project = crate::reactor_project::ReactorProject::new_proposed(
            project_id, reactor_id, name, scale, enrichment, balance_cfg,
        );
        self.reactor_projects.push(project);
        project_id
    }

    pub fn find_reactor_project(
        &self,
        id: crate::reactor_project::ReactorProjectId,
    ) -> Option<&crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter().find(|rp| rp.project_id == id)
    }

    pub fn find_reactor_project_mut(
        &mut self,
        id: crate::reactor_project::ReactorProjectId,
    ) -> Option<&mut crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter_mut().find(|rp| rp.project_id == id)
    }

    /// Visible reactor projects (everything not Proposed and not
    /// retired). Mirrors `visible_engine_projects`.
    pub fn visible_reactor_projects(
        &self,
    ) -> impl Iterator<Item = (usize, &crate::reactor_project::ReactorProject)> {
        self.reactor_projects.iter().enumerate().filter(|(_, rp)|
            !matches!(rp.status, crate::reactor_project::ReactorDesignStatus::Proposed { .. })
                && !rp.retired
        )
    }

    /// Reactor projects that are usable in a rocket — anything past
    /// design, i.e. Testing or Revising. Discovered un-revised flaws
    /// don't block installation; they fly with the reactor.
    pub fn installable_reactor_projects(
        &self,
    ) -> impl Iterator<Item = &crate::reactor_project::ReactorProject> {
        self.reactor_projects.iter().filter(|rp| !rp.retired && matches!(
            rp.status,
            crate::reactor_project::ReactorDesignStatus::Testing { .. }
            | crate::reactor_project::ReactorDesignStatus::Revising { .. },
        ))
    }

    /// Start a new rocket design project. Returns the event if successful.
    /// The engines a design uses, as "Name Rev N" with a count each, in
    /// stage order: what the Rockets tab lists under a project.
    pub fn engine_usage(&self, design: &RocketDesign) -> Vec<(String, u32)> {
        let mut seen: Vec<(String, u32)> = Vec::new();
        for stage in design.stage_groups.iter().flatten() {
            let rev = self.engine_projects.iter()
                .find(|ep| ep.design.id == stage.engine.id)
                .map(|ep| ep.revision)
                .or_else(|| self.contracted_engines.iter()
                    .find(|ce| ce.design.id == stage.engine.id)
                    .map(|_| 0))
                .unwrap_or(0);
            let key = format!("{} Rev {}", stage.engine.name, rev);
            if let Some(entry) = seen.iter_mut().find(|(k, _)| k == &key) {
                entry.1 += stage.engine_count;
            } else {
                seen.push((key, stage.engine_count));
            }
        }
        seen
    }

    pub fn start_rocket_project(&mut self, design: RocketDesign, balance_cfg: &BalanceConfig) -> Option<GameEvent> {
        let project_id = RocketProjectId(self.next_rocket_project_id);
        self.next_rocket_project_id += 1;
        let name = design.name.clone();
        let project = RocketProject::new(project_id, design, balance_cfg);
        self.rocket_projects.push(project);
        Some(GameEvent::project(crate::project::ProjectKind::Rocket, name, ProjectEvent::DesignStarted))
    }

    /// Rocket projects that should be visible in the Rockets pane —
    /// everything the player hasn't retired. Mirrors
    /// `visible_engine_projects`; rocket projects have no `Proposed`
    /// status, so retirement is the only thing it hides.
    pub fn visible_rocket_projects(&self) -> impl Iterator<Item = (usize, &RocketProject)> {
        self.rocket_projects.iter().enumerate()
            .filter(|(_, rp)| !rp.is_proposed() && !rp.retired)
    }
}

// ── Projects across the three lists ──────────────────────────────────
//
// Engines, rockets and reactors are three `Vec`s of one generic
// `DesignProject`. Everything that doesn't care which kind it is
// holding goes through `ProjectRef` and the `ProjectCore` view below,
// so adding a fourth kind means adding a list and an arm here, not a
// fourth copy of every method.

impl Company {
    /// The project `r` points at, whichever list it lives in.
    pub fn project(&self, r: ProjectRef) -> Option<&dyn ProjectCore> {
        match r {
            ProjectRef::Engine(id) => self.engine_projects.iter()
                .find(|p| p.project_id == id).map(|p| p as &dyn ProjectCore),
            ProjectRef::Rocket(id) => self.rocket_projects.iter()
                .find(|p| p.project_id == id).map(|p| p as &dyn ProjectCore),
            ProjectRef::Reactor(id) => self.reactor_projects.iter()
                .find(|p| p.project_id == id).map(|p| p as &dyn ProjectCore),
        }
    }

    pub fn project_mut(&mut self, r: ProjectRef) -> Option<&mut dyn ProjectCore> {
        match r {
            ProjectRef::Engine(id) => self.engine_projects.iter_mut()
                .find(|p| p.project_id == id).map(|p| p as &mut dyn ProjectCore),
            ProjectRef::Rocket(id) => self.rocket_projects.iter_mut()
                .find(|p| p.project_id == id).map(|p| p as &mut dyn ProjectCore),
            ProjectRef::Reactor(id) => self.reactor_projects.iter_mut()
                .find(|p| p.project_id == id).map(|p| p as &mut dyn ProjectCore),
        }
    }

    /// Position of `r` in its kind's list, for the few APIs still
    /// addressed by index (build orders, auto-build targets).
    pub fn list_index(&self, r: ProjectRef) -> Option<usize> {
        match r {
            ProjectRef::Engine(id) => self.engine_projects.iter().position(|p| p.project_id == id),
            ProjectRef::Rocket(id) => self.rocket_projects.iter().position(|p| p.project_id == id),
            ProjectRef::Reactor(id) => self.reactor_projects.iter().position(|p| p.project_id == id),
        }
    }

    /// Every project: engines, then rockets, then reactors — the order
    /// the daily tick and the auto-assigner have always walked them.
    pub fn projects(&self) -> impl Iterator<Item = &dyn ProjectCore> {
        self.engine_projects.iter().map(|p| p as &dyn ProjectCore)
            .chain(self.rocket_projects.iter().map(|p| p as &dyn ProjectCore))
            .chain(self.reactor_projects.iter().map(|p| p as &dyn ProjectCore))
    }

    pub fn projects_mut(&mut self) -> impl Iterator<Item = &mut dyn ProjectCore> {
        self.engine_projects.iter_mut().map(|p| p as &mut dyn ProjectCore)
            .chain(self.rocket_projects.iter_mut().map(|p| p as &mut dyn ProjectCore))
            .chain(self.reactor_projects.iter_mut().map(|p| p as &mut dyn ProjectCore))
    }

    /// The projects of one kind a pane shows — not `Proposed`, not
    /// retired — in list order. Pane selections index this.
    pub fn visible_projects(&self, kind: ProjectKind) -> Box<dyn Iterator<Item = &dyn ProjectCore> + '_> {
        let all: Box<dyn Iterator<Item = &dyn ProjectCore>> = match kind {
            ProjectKind::Engine => Box::new(self.engine_projects.iter().map(|p| p as &dyn ProjectCore)),
            ProjectKind::Rocket => Box::new(self.rocket_projects.iter().map(|p| p as &dyn ProjectCore)),
            ProjectKind::Reactor => Box::new(self.reactor_projects.iter().map(|p| p as &dyn ProjectCore)),
        };
        Box::new(all.filter(|p| !p.is_proposed() && !p.retired()))
    }

    /// Put an idle engineering team on `r`. False if none is idle or
    /// `r` is gone.
    pub fn add_team(&mut self, r: ProjectRef) -> bool {
        if self.unassigned_team_count() == 0 {
            return false;
        }
        match self.project_mut(r) {
            Some(p) => {
                *p.teams_assigned_mut() += 1;
                true
            }
            None => false,
        }
    }

    /// Take one team off `r`. False if it had none or is gone.
    pub fn remove_team(&mut self, r: ProjectRef) -> bool {
        match self.project_mut(r) {
            Some(p) if p.teams_assigned() > 0 => {
                *p.teams_assigned_mut() -= 1;
                true
            }
            _ => false,
        }
    }

    /// Move one team from the busiest *other* project onto `target`.
    /// Returns the donor's name. Ties go to the earliest in
    /// engines-rockets-reactors list order.
    pub fn steal_team_to(&mut self, target: ProjectRef) -> Option<String> {
        self.project(target)?;
        let (donor, name) = self.projects()
            .filter(|p| p.project_ref() != target && p.teams_assigned() > 0)
            .fold(None::<(ProjectRef, u32, String)>, |best, p| match best {
                Some(b) if p.teams_assigned() <= b.1 => Some(b),
                _ => Some((p.project_ref(), p.teams_assigned(), p.name().to_string())),
            })
            .map(|(r, _, name)| (r, name))?;
        *self.project_mut(donor)?.teams_assigned_mut() -= 1;
        *self.project_mut(target)?.teams_assigned_mut() += 1;
        Some(name)
    }

    /// Start a revision on `r`: what got queued, or `None` if `r` is
    /// gone, not in Testing, or has nothing to revise.
    pub fn start_revision(&mut self, r: ProjectRef) -> Option<RevisionPlan> {
        self.project_mut(r)?.begin_revision()
    }

    /// Promote a `Proposed` draft to `InDesign`. The project's name on
    /// success (for logging); `None` if `r` is gone or isn't a draft.
    pub fn promote_proposed(&mut self, r: ProjectRef) -> Option<String> {
        let p = self.project_mut(r)?;
        if !p.is_proposed() {
            return None;
        }
        p.promote_to_in_design();
        Some(p.name().to_string())
    }

    /// Delete a `Proposed` draft. Silently no-ops on anything else —
    /// defensive, so a stale id can never delete real work.
    pub fn delete_proposed(&mut self, r: ProjectRef) {
        match r {
            ProjectRef::Engine(id) =>
                self.engine_projects.retain(|p| !(p.project_id == id && p.is_proposed())),
            ProjectRef::Rocket(id) =>
                self.rocket_projects.retain(|p| !(p.project_id == id && p.is_proposed())),
            ProjectRef::Reactor(id) =>
                self.reactor_projects.retain(|p| !(p.project_id == id && p.is_proposed())),
        }
    }

    /// Flip auto-revise on `r`; the project's name and the new setting.
    pub fn toggle_auto_revise(&mut self, r: ProjectRef) -> Option<(String, bool)> {
        let p = self.project_mut(r)?;
        let flag = p.auto_revise_mut();
        *flag = !*flag;
        let on = *flag;
        Some((p.name().to_string(), on))
    }

    /// Put every idle engineering team to work, each on the
    /// least-staffed committed project at that moment. An idle team is
    /// pure waste — testing and revising consume work just as designing
    /// does — so the only real decision is which project to *move* a
    /// team to, and that stays manual (`+` steals from the busiest).
    ///
    /// Drafts (`Proposed`) and retired designs are skipped. Ties resolve
    /// to the earliest in engines-rockets-reactors list order.
    pub fn auto_assign_idle_engineering_teams(&mut self) {
        while self.unassigned_team_count() > 0 {
            let target = self.projects()
                .filter(|p| !p.is_proposed() && !p.retired())
                .min_by_key(|p| p.teams_assigned())
                .map(|p| p.project_ref());
            // Nothing to work on — the teams stay idle and the
            // Overview's next-steps panel prompts for a project.
            let Some(target) = target else { break };
            if !self.add_team(target) {
                break;
            }
        }
    }
}
