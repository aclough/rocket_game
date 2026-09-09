//! The engineering day: every project's teams do their work and the
//! outcomes the game state has to finish are set aside.

use super::*;

/// What one day of R&D produced — see [`Company::tick_daily_research`].
pub struct ResearchTick {
    pub events: Vec<GameEvent>,
    /// Engine projects whose design completed today.
    pub newly_designed_engines: Vec<EngineProjectId>,
    /// (engine project, deficiency) revision attempts to resolve.
    pub tech_def_attempts: Vec<(EngineProjectId, crate::technology::TechDeficiencyId)>,
    /// Reactor projects whose design completed today.
    pub newly_designed_reactors: Vec<crate::reactor_project::ReactorProjectId>,
    /// (reactor project, deficiency) revision attempts to resolve.
    pub reactor_tech_def_attempts: Vec<(crate::reactor_project::ReactorProjectId, crate::technology::TechDeficiencyId)>,
}

/// Turn one day's work on a project into game events, and set aside
/// the two outcomes the game state has to finish: designs that just
/// completed (they inherit their technology's deficiencies) and
/// deficiency revision attempts (they roll against the technology
/// table). Testing cycles completing are bookkeeping, not news.
fn report_work<D: Designable>(
    project: &DesignProject<D>,
    work_events: Vec<WorkEvent>,
    events: &mut Vec<GameEvent>,
    newly_designed: &mut Vec<D::Id>,
    tech_def_attempts: &mut Vec<(D::Id, crate::technology::TechDeficiencyId)>,
) {
    for we in work_events {
        let event = match we {
            WorkEvent::DesignComplete => {
                newly_designed.push(project.project_id);
                ProjectEvent::DesignComplete
            }
            WorkEvent::TestingCycleComplete => continue,
            WorkEvent::FlawDiscovered { flaw_description } =>
                ProjectEvent::FlawDiscovered { description: flaw_description },
            WorkEvent::RevisionComplete => ProjectEvent::RevisionComplete,
            WorkEvent::ImprovementDiscovered { description } =>
                ProjectEvent::ImprovementDiscovered { description },
            WorkEvent::ImprovementActualized { description } =>
                ProjectEvent::ImprovementActualized { description },
            WorkEvent::TechDeficiencyAttempted { deficiency_id } => {
                tech_def_attempts.push((project.project_id, deficiency_id));
                continue;
            }
        };
        events.push(GameEvent::project(D::KIND, project.design.name(), event));
    }
}

impl Company {
    /// One day of R&D across this company's engine / rocket / reactor
    /// project lists: daily work, flaw discovery, revisions, and NRE
    /// accrual. Extracted from `advance_day` (M3 hygiene) so scripted
    /// competitors can eventually run the same loop. Tech-deficiency
    /// *resolution* stays with the game state — it needs the world's
    /// technology table — so the attempt lists ride back in the report.
    pub fn tick_daily_research(
        &mut self,
        rng: &mut rand::rngs::StdRng,
        balance_cfg: &BalanceConfig,
    ) -> ResearchTick {
        let mut events: Vec<GameEvent> = Vec::new();
        let mut newly_designed_engines = Vec::new();
        let mut tech_def_attempts = Vec::new();
        // Reactor equivalents (mirror the engine tech-deficiency flow).
        let mut newly_designed_reactors = Vec::new();
        let mut reactor_tech_def_attempts = Vec::new();
        let next_flaw_id = &mut self.next_flaw_id;

        for project in self.engine_projects.iter_mut() {
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            report_work(project, work_events, &mut events, &mut newly_designed_engines, &mut tech_def_attempts);
        }
        for project in self.rocket_projects.iter_mut() {
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            // Rockets carry no technology, so nothing lands in these.
            report_work(project, work_events, &mut events, &mut Vec::new(), &mut Vec::new());
        }
        for project in self.reactor_projects.iter_mut() {
            let work_events = project.apply_daily_work(rng, next_flaw_id, balance_cfg);
            report_work(project, work_events, &mut events, &mut newly_designed_reactors, &mut reactor_tech_def_attempts);
        }

        // Accumulate NRE (engineering salary) on active projects
        let daily_salary = balance_cfg.costs.daily_engineering_salary();
        for project in self.projects_mut() {
            let teams = project.teams_assigned();
            if teams > 0 {
                *project.nre_cost_mut() += teams as f64 * daily_salary;
            }
        }

        ResearchTick {
            events,
            newly_designed_engines,
            tech_def_attempts,
            newly_designed_reactors,
            reactor_tech_def_attempts,
        }
    }
}
