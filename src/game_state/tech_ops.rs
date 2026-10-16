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
    fn technology_ids(&self) -> &[TechnologyId];
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId>;
    fn complexity_mut(&mut self) -> &mut u32;
    fn build_time_multiplier_mut(&mut self) -> &mut f64;
    fn material_cost_multiplier_mut(&mut self) -> &mut f64;
    fn test_difficulty_mut(&mut self) -> &mut u32;
    fn apply_deficiency(&mut self, kind: &TechDeficiencyKind, dir: Direction);
}

impl TechProject for EngineProject {
    type Id = EngineProjectId;
    fn list(company: &mut Company) -> &mut Vec<Self> { &mut company.engine_projects }
    const KIND: ProjectKind = ProjectKind::Engine;
    fn id(&self) -> Self::Id { self.project_id }
    fn name(&self) -> &str { &self.design.name }
    fn technology_ids(&self) -> &[TechnologyId] { &self.technology_ids }
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId> { &mut self.tech_deficiency_ids }
    fn complexity_mut(&mut self) -> &mut u32 { &mut self.complexity }
    fn build_time_multiplier_mut(&mut self) -> &mut f64 { &mut self.build_time_multiplier }
    fn material_cost_multiplier_mut(&mut self) -> &mut f64 { &mut self.material_cost_multiplier }
    fn test_difficulty_mut(&mut self) -> &mut u32 { &mut self.test_difficulty }
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
    fn technology_ids(&self) -> &[TechnologyId] { &self.technology_ids }
    fn tech_deficiency_ids_mut(&mut self) -> &mut Vec<TechDeficiencyId> { &mut self.tech_deficiency_ids }
    fn complexity_mut(&mut self) -> &mut u32 { &mut self.complexity }
    fn build_time_multiplier_mut(&mut self) -> &mut f64 { &mut self.build_time_multiplier }
    fn material_cost_multiplier_mut(&mut self) -> &mut f64 { &mut self.material_cost_multiplier }
    fn test_difficulty_mut(&mut self) -> &mut u32 { &mut self.test_difficulty }
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
        let solved_before = self.player_company.solved_deficiencies.clone();
        let mut newly_solved = Vec::new();
        for (pid, def_id) in attempts {
            let Some(project) = P::list(&mut self.player_company).iter_mut().find(|p| p.id() == pid) else {
                continue;
            };
            // The deficiency belongs to one of the project's technologies.
            let tech_ids = project.technology_ids().to_vec();
            let Some(tech) = self.technologies.iter_mut()
                .find(|t| tech_ids.contains(&t.id) && t.deficiencies.iter().any(|d| d.id == def_id))
            else { continue };
            let Some(def) = tech.deficiencies.iter_mut().find(|d| d.id == def_id) else { continue };
            let already_solved = solved_before.contains(&def_id);
            let name = project.name().to_string();
            let def_desc = format!("{}: {}", def.description, def.kind);

            if technology::attempt_solve(def, already_solved, &mut self.seed.contingent_rng, &self.balance.technology) {
                newly_solved.push(def_id);
                project.tech_deficiency_ids_mut().retain(|id| *id != def_id);
                match &def.kind {
                    TechDeficiencyKind::ComplexityPenalty(n) => {
                        let c = project.complexity_mut();
                        *c = c.saturating_sub(*n);
                    }
                    TechDeficiencyKind::BuildTimePenalty(m) => *project.build_time_multiplier_mut() /= m,
                    TechDeficiencyKind::MaterialCostPenalty(m) => *project.material_cost_multiplier_mut() /= m,
                    TechDeficiencyKind::HardToTest(k) => {
                        let t = project.test_difficulty_mut();
                        *t = t.saturating_sub(*k);
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
        self.player_company.solved_deficiencies.extend(newly_solved);
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
        let solved = self.player_company.solved_deficiencies.clone();
        for pid in newly_designed {
            let Some(project) = P::list(&mut self.player_company).iter_mut().find(|p| p.id() == pid) else {
                continue;
            };
            // Every technology the design is on, each contributing what
            // this company has not solved yet: a solved deficiency is
            // retired for every design it makes from here.
            let tech_ids = project.technology_ids().to_vec();
            let mut all_ids = Vec::new();
            for tech in self.technologies.iter().filter(|t| tech_ids.contains(&t.id)) {
                let pending = tech.unsolved_for(&solved);
                for def in &pending {
                    match &def.kind {
                        TechDeficiencyKind::ComplexityPenalty(n) => *project.complexity_mut() += n,
                        TechDeficiencyKind::BuildTimePenalty(m) => *project.build_time_multiplier_mut() *= m,
                        TechDeficiencyKind::MaterialCostPenalty(m) => *project.material_cost_multiplier_mut() *= m,
                        TechDeficiencyKind::HardToTest(k) => *project.test_difficulty_mut() += k,
                        kind => project.apply_deficiency(kind, Direction::Apply),
                    }
                }
                all_ids.extend(pending.iter().map(|d| d.id));
                let desc: Vec<String> = pending.iter()
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
            *project.tech_deficiency_ids_mut() = all_ids;
        }
        self.emit_all(events, news);
    }
}
