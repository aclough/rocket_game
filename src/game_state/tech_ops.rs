//! Technology-deficiency resolution for R&D projects.
//!
//! A project built on an experimental technology inherits that
//! technology's deficiencies when its design completes, and each
//! revision attempt on a deficiency rolls against its solvability. Both
//! halves need the world's technology table, which is why they live on
//! `GameState` rather than in `Company::tick_daily_research`; the tick
//! reports what happened and this module resolves it.
//!
//! Engines and reactors run the identical flow; only which design stat a
//! deficiency kind touches differs, and that lives on the design
//! (`apply_deficiency`).

use crate::company::Company;
use crate::engine_project::{EngineProject, EngineProjectId};
use crate::event::{GameEvent, ProjectEvent};
use crate::project::{Direction, ProjectKind};
use crate::reactor_project::{ReactorProject, ReactorProjectId};
use crate::technology::{self, TechDeficiencyId, TechDeficiencyKind, TechnologyId};

use super::GameState;

/// The slice of a project the deficiency flow needs. Folds into the
/// shared project trait in a later step of `17_1_RD_PIPELINE.md`.
pub(super) trait TechProject {
    type Id: Copy + PartialEq;
    /// Where this kind of project lives on a company.
    fn list(company: &mut Company) -> &mut Vec<Self> where Self: Sized;
    const KIND: ProjectKind;

    fn id(&self) -> Self::Id;
    fn name(&self) -> &str;
    fn technology_id(&self) -> Option<TechnologyId>;
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId>;
    fn complexity_mut(&mut self) -> &mut u32;
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction);
}

impl TechProject for EngineProject {
    type Id = EngineProjectId;
    fn list(company: &mut Company) -> &mut Vec<Self> { &mut company.engine_projects }
    const KIND: ProjectKind = ProjectKind::Engine;
    fn id(&self) -> Self::Id { self.project_id }
    fn name(&self) -> &str { &self.design.name }
    fn technology_id(&self) -> Option<TechnologyId> { self.technology_id }
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId> { &mut self.tech_deficiency_ids }
    fn complexity_mut(&mut self) -> &mut u32 { &mut self.complexity }
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction) {
        self.design.apply_deficiency(kind, dir);
    }
}

impl TechProject for ReactorProject {
    type Id = ReactorProjectId;
    fn list(company: &mut Company) -> &mut Vec<Self> { &mut company.reactor_projects }
    const KIND: ProjectKind = ProjectKind::Reactor;
    fn id(&self) -> Self::Id { self.project_id }
    fn name(&self) -> &str { &self.design.name }
    fn technology_id(&self) -> Option<TechnologyId> { self.technology_id }
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId> { &mut self.tech_deficiency_ids }
    fn complexity_mut(&mut self) -> &mut u32 { &mut self.complexity }
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction) {
        self.design.apply_deficiency(kind, dir);
    }
}

impl GameState {
    /// Roll every deficiency revision attempt the research tick
    /// reported. A success takes the deficiency off the project and
    /// reverts its stat effect; a failure is reported with the running
    /// attempt count's hint, if that count has earned one. A deficiency
    /// already solved on another project is much easier to solve again.
    pub(super) fn resolve_tech_attempts<P: TechProject>(
        &mut self,
        attempts: Vec<(P::Id, TechDeficiencyId)>,
        events: &mut Vec<GameEvent>,
    ) {
        let mut news = Vec::new();
        for (pid, def_id) in attempts {
            let Some(project) = P::list(&mut self.player_company).iter_mut().find(|p| p.id() == pid) else {
                continue;
            };
            let Some(tech_id) = project.technology_id() else { continue };
            let Some(tech) = self.technologies.iter_mut().find(|t| t.id == tech_id) else { continue };
            let Some(def) = tech.deficiencies.iter_mut().find(|d| d.id == def_id) else { continue };
            let already_solved = def.solved;
            let name = project.name().to_string();
            let def_desc = format!("{}: {}", def.description, def.kind);

            if technology::attempt_solve(def, already_solved, &mut self.seed.contingent_rng) {
                project.tech_deficiency_ids_mut().retain(|id| *id != def_id);
                match &def.kind {
                    TechDeficiencyKind::ComplexityPenalty(n) => {
                        let c = project.complexity_mut();
                        *c = c.saturating_sub(*n);
                    }
                    kind => project.apply_deficiency(kind, Direction::Revert),
                }
                news.push(GameEvent::project(P::KIND, name, ProjectEvent::RevisionComplete));
            } else {
                let description = match technology::failure_hint(def.total_attempts) {
                    Some(hint) => format!("failed to resolve {}. {}", def_desc, hint),
                    None => format!("failed to resolve {}", def_desc),
                };
                news.push(GameEvent::project(
                    P::KIND, name, ProjectEvent::TechDeficiencyUnresolved { description },
                ));
            }
        }
        self.emit_all(events, news);
    }

    /// Put a technology's deficiencies onto every project whose design
    /// completed today: stat penalties onto the design, complexity onto
    /// the project, and the ids onto the project's to-solve list.
    pub(super) fn apply_new_design_deficiencies<P: TechProject>(
        &mut self,
        newly_designed: Vec<P::Id>,
        events: &mut Vec<GameEvent>,
    ) {
        let mut news = Vec::new();
        for pid in newly_designed {
            let Some(project) = P::list(&mut self.player_company).iter_mut().find(|p| p.id() == pid) else {
                continue;
            };
            let Some(tech_id) = project.technology_id() else { continue };
            let Some(tech) = self.technologies.iter().find(|t| t.id == tech_id) else { continue };
            for def in &tech.deficiencies {
                match &def.kind {
                    TechDeficiencyKind::ComplexityPenalty(n) => *project.complexity_mut() += n,
                    kind => project.apply_deficiency(kind, Direction::Apply),
                }
            }
            *project.tech_deficiency_ids_mut() = tech.deficiencies.iter().map(|d| d.id).collect();
            let desc: Vec<String> = tech.deficiencies.iter()
                .map(|d| format!("{}: {}", d.description, d.kind))
                .collect();
            if !desc.is_empty() {
                news.push(GameEvent::project(
                    P::KIND,
                    project.name(),
                    ProjectEvent::TechDeficienciesFound {
                        tech_name: tech.name.clone(),
                        deficiencies: desc.join(", "),
                    },
                ));
            }
        }
        self.emit_all(events, news);
    }
}
