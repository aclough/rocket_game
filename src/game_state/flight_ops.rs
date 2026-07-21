//! Launch and flight operations: manifest assembly, launches,
//! in-transit flight advancement, and parked-spacecraft ops
//! (fly / dock / undock).


use crate::engine_project::EngineSource;
use crate::flight::{Flight, FlightId, FlightStatus, Payload};
use crate::event::GameEvent;
use crate::launch::{self, LaunchRecord, LaunchOutcome};
use crate::rocket::RocketId;

use super::*;

impl GameState {
    /// Assemble a launch manifest from contract picks and spacecraft
    /// inventory items: resolves the shared destination (all picked
    /// contracts must agree; defaults to LEO with no contract picks),
    /// builds `ContractDelivery` payloads, and takes each picked
    /// inventory rocket, instantiating it as a `Spacecraft` payload
    /// deployed at the destination. Validates everything before
    /// consuming inventory, so on error nothing is taken. An empty
    /// manifest becomes a zero-mass test launch.
    ///
    /// `contract_indices` index into `player_company.active_contracts`.
    pub fn build_launch_payloads(
        &mut self,
        contract_indices: &[usize],
        spacecraft_item_ids: &[crate::manufacturing::InventoryItemId],
    ) -> Result<(String, Vec<Payload>), ManifestError> {
        // Destination must agree across picked contracts.
        let mut destination: Option<String> = None;
        for &i in contract_indices {
            let dest = self.player_company.active_contracts[i].destination.clone();
            match &destination {
                None => destination = Some(dest),
                Some(d) if d == &dest => {}
                Some(d) => {
                    return Err(ManifestError::ConflictingDestinations {
                        first: d.clone(),
                        second: dest,
                    });
                }
            }
        }
        let destination = destination.unwrap_or_else(|| "leo".to_string());

        // Validate spacecraft picks before consuming any inventory.
        for &item_id in spacecraft_item_ids {
            let inv = self.player_company.manufacturing.inventory.rockets.iter()
                .find(|r| r.item_id == item_id)
                .ok_or(ManifestError::SpacecraftMissing)?;
            if !self.player_company.rocket_projects.iter()
                .any(|rp| rp.project_id == inv.rocket_project_id)
            {
                return Err(ManifestError::PayloadProjectMissing);
            }
        }

        let mut payloads: Vec<Payload> = Vec::new();
        for &i in contract_indices {
            let c = &self.player_company.active_contracts[i];
            payloads.push(Payload::ContractDelivery {
                contract_id: c.id,
                payload_kg: c.payload_kg,
            });
        }

        // Take picked inventory rockets and pack them as Spacecraft
        // payloads with full propellant. Nested payload mass is 0 (no
        // recursive picking yet).
        for &item_id in spacecraft_item_ids {
            let inv_rocket = self.player_company.manufacturing.inventory
                .take_rocket(item_id)
                .expect("validated above");
            let design = self.player_company.rocket_projects.iter()
                .find(|rp| rp.project_id == inv_rocket.rocket_project_id)
                .expect("validated above")
                .design.clone();
            let rocket_id = crate::rocket::RocketId(self.next_rocket_id);
            self.next_rocket_id += 1;
            let rocket = design.instantiate(rocket_id, "earth_surface", 0.0);
            payloads.push(Payload::Spacecraft {
                deploy_at: Some(destination.clone()),
                design,
                rocket,
                nested_payloads: vec![],
                rocket_project_id: inv_rocket.rocket_project_id,
                name: inv_rocket.rocket_name.clone(),
            });
        }

        if payloads.is_empty() {
            payloads.push(Payload::TestMass { mass_kg: 0.0 });
        }

        Ok((destination, payloads))
    }

    /// Launch a rocket carrying a manifest of payloads.
    /// `rocket_item_id` identifies the InventoryRocket to use as the carrier.
    /// `payloads` is the full manifest — any combination of contract
    /// deliveries, test masses, and nested Spacecraft. The caller is
    /// responsible for already having taken any nested-rocket inventory
    /// items out of inventory and packed them into Spacecraft payloads.
    /// Returns events; on catastrophic failure, also a LaunchRecord. On
    /// success/partial success, the rocket enters transit and resolves on
    /// arrival.
    pub fn launch_rocket(
        &mut self,
        rocket_item_id: crate::manufacturing::InventoryItemId,
        destination: &str,
        payloads: Vec<Payload>,
        persist: bool,
    ) -> Option<(Vec<GameEvent>, Option<LaunchRecord>)> {
        let total_payload_kg: f64 = payloads.iter().map(|p| p.mass_kg()).sum();

        // Take the rocket from inventory
        let inv_rocket = self.player_company.manufacturing.inventory.take_rocket(rocket_item_id)?;

        // Find the rocket project for this rocket
        let rp = self.player_company.rocket_projects.iter()
            .find(|rp| rp.project_id == inv_rocket.rocket_project_id)?;

        // Use snapshotted rocket flaws from the inventory item
        let rocket_flaws = &inv_rocket.rocket_flaws;

        // Simulate flaw activation at launch
        let sim = launch::simulate_launch(
            &rp.design,
            destination,
            total_payload_kg,
            &self.player_company.engine_projects,
            rocket_flaws,
            &self.player_company.contracted_engines,
            &mut self.seed.contingent_rng,
        );

        let mut events = Vec::new();

        // Mark activated flaws as discovered on engine projects
        for (engine_id, indices) in &sim.engine_flaw_discoveries {
            if let Some(ep) = self.player_company.engine_projects.iter_mut()
                .find(|ep| ep.design.id == *engine_id)
            {
                for &idx in indices {
                    if idx < ep.flaws.len() {
                        ep.flaws[idx].discovered = true;
                        let evt = GameEvent::FlawDiscovered {
                            engine_name: ep.design.name.clone(),
                            flaw_description: ep.flaws[idx].description.clone(),
                        };
                        self.event_log.push(self.date, evt.clone());
                        events.push(evt);
                    }
                }
            }
        }

        // Mark activated flaws as discovered on contracted engines
        for (source, indices) in &sim.contracted_flaw_discoveries {
            if let EngineSource::Contracted(ce_id) = source {
                if let Some(ce) = self.player_company.contracted_engines.iter_mut()
                    .find(|ce| ce.id == *ce_id)
                {
                    for &idx in indices {
                        if idx < ce.flaws.len() {
                            ce.flaws[idx].discovered = true;
                        }
                    }
                }
            }
        }

        // Mark activated flaws as discovered on rocket project
        if let Some(rp_mut) = self.player_company.rocket_projects.iter_mut()
            .find(|rp| rp.project_id == inv_rocket.rocket_project_id)
        {
            for &idx in &sim.rocket_flaw_discoveries {
                if idx < rp_mut.flaws.len() {
                    rp_mut.flaws[idx].discovered = true;
                    let evt = GameEvent::RocketFlawDiscovered {
                        rocket_name: rp_mut.design.name.clone(),
                        flaw_description: rp_mut.flaws[idx].description.clone(),
                    };
                    self.event_log.push(self.date, evt.clone());
                    events.push(evt);
                }
            }
        }


        // Update launch tracking
        self.player_company.last_launch_date = Some(self.date);

        // Catastrophic failure at launch — resolve immediately. The carrier
        // and all nested Spacecraft payloads are destroyed (the `payloads`
        // Vec is dropped here — by user spec, nothing returns to inventory).
        // All on-manifest contracts are forfeited.
        if matches!(sim.outcome, LaunchOutcome::Failure { .. }) {
            let mut contract_id_for_record: Option<crate::contract::ContractId> = None;
            let manifest_contract_ids: Vec<crate::contract::ContractId> = payloads.iter()
                .filter_map(|p| match p {
                    Payload::ContractDelivery { contract_id, .. } => Some(*contract_id),
                    _ => None,
                })
                .collect();
            if let Some(first) = manifest_contract_ids.first() {
                contract_id_for_record = Some(*first);
            }

            let severity = self.manifest_failure_severity(&manifest_contract_ids);
            self.player_company.reputation.on_launch_failure(&self.balance.reputation, severity);

            for cid in &manifest_contract_ids {
                if let Some(ci) = self.player_company.active_contracts.iter()
                    .position(|c| c.id == *cid)
                {
                    self.player_company.active_contracts.remove(ci);
                }
            }

            let reason = match &sim.outcome {
                LaunchOutcome::Failure { reason } => reason.clone(),
                _ => unreachable!(),
            };
            let evt = GameEvent::LaunchFailure {
                rocket_name: inv_rocket.rocket_name.clone(),
                reason: reason.clone(),
            };
            self.event_log.push(self.date, evt.clone());
            events.push(evt);

            let record = LaunchRecord {
                launch_date: self.date,
                rocket_name: inv_rocket.rocket_name,
                contract_id: contract_id_for_record,
                destination: destination.to_string(),
                payload_kg: total_payload_kg,
                outcome: sim.outcome,
                flaws_activated: sim.flaws_activated,
            };
            self.player_company.launch_history.push(record.clone());
            self.speed = GameSpeed::Paused;
            return Some((events, Some(record)));
        }

        // Success or partial failure — create a flight in transit.
        // Refuse to launch if the active group's engines have no
        // electrical power available at takeoff (e.g. ion stage with no
        // panels). Chemical engines always have nominal thrust regardless.
        let avail_power_at_takeoff = sim.degraded_design.power_for_engines_w(1.0);
        let first_group_thrust = sim.degraded_design
            .group_effective_thrust_n(0, avail_power_at_takeoff);

        let path = crate::location::DELTA_V_MAP
            .shortest_path_for_rocket(
                "earth_surface", destination, &sim.degraded_design, total_payload_kg,
            );
        // Build the route using the power-aware path so per-leg burn
        // times reflect each leg's sun-distance (Phase 2b).
        let route = if first_group_thrust <= 0.0 {
            Vec::new()
        } else {
            match path {
                Some((path, _)) => {
                    let sim_rocket = sim.degraded_design.instantiate(
                        crate::rocket::RocketId(0),
                        "earth_surface",
                        total_payload_kg,
                    );
                    crate::flight::build_route_for_rocket(
                        &path, &sim.degraded_design, &sim_rocket, total_payload_kg,
                    )
                }
                None => vec![],
            }
        };

        let flight_id = FlightId(self.next_flight_id);
        self.next_flight_id += 1;

        // Instantiate a Rocket with per-stage propellant tracking
        let rocket_instance_id = RocketId(self.next_rocket_id);
        self.next_rocket_id += 1;
        let rocket_instance = sim.degraded_design.instantiate(
            rocket_instance_id, "earth_surface", total_payload_kg,
        );

        let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);

        let dest_display = crate::contract::destination_display_name(destination);

        let flight = Flight {
            id: flight_id,
            // launch_rocket is the player's pad; competitor flights
            // stay abstract until they get a real launch path.
            company: crate::flight::CompanyRef::Player,
            rocket_name: inv_rocket.rocket_name.clone(),
            rocket_project_id: inv_rocket.rocket_project_id,
            design: sim.degraded_design,
            rocket: rocket_instance,
            payloads,
            current_location: "earth_surface".to_string(),
            route,
            current_leg: 0,
            leg_days_remaining: leg_days,
            status: FlightStatus::InTransit,
            flaws_activated: sim.flaws_activated,
            launch_date: self.date,
            persist,
            launch_partial: matches!(sim.outcome, LaunchOutcome::PartialFailure { .. }),
            flaw_rolled_groups: sim.flaw_rolled_groups,
            reactor_flaws_rolled: false,
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: inv_rocket.rocket_name,
            destination: dest_display.to_string(),
        };
        self.event_log.push(self.date, evt.clone());
        events.push(evt);

        self.speed = GameSpeed::Paused;

        Some((events, None))
    }

    /// Process daily flight advancement. Returns events generated.
    pub(super) fn advance_flights(&mut self) -> Vec<GameEvent> {
        use rand::Rng;
        use crate::engine::EngineId;
        use crate::flaw::{FlawConsequence, FlawTrigger};
        use crate::engine_project::EngineSource;
        use crate::rocket_project::RocketProjectId;

        let mut events = Vec::new();
        let mut arrived_indices = Vec::new();
        let mut stranded_indices = Vec::new();
        // Flights destroyed mid-flight by a catastrophic stage loss.
        let mut lost_indices: Vec<usize> = Vec::new();

        // Snapshot engine flaws keyed by engine_id for lookup during flight iteration.
        // Each entry: (engine_id, engine_name, flaw_index_in_project, flaw_data, source)
        struct FlawRef {
            engine_id: EngineId,
            engine_name: String,
            activation_chance: f64,
            consequence: FlawConsequence,
            description: String,
            source: EngineSource,
            flaw_index: usize,
        }
        let mut flaw_table: Vec<FlawRef> = Vec::new();
        for ep in &self.player_company.engine_projects {
            let source = EngineSource::PlayerDesign(ep.project_id);
            for (fi, flaw) in ep.flaws.iter().enumerate() {
                flaw_table.push(FlawRef {
                    engine_id: ep.design.id,
                    engine_name: ep.design.name.clone(),
                    activation_chance: flaw.activation_chance,
                    consequence: flaw.consequence.clone(),
                    description: flaw.description.clone(),
                    source,
                    flaw_index: fi,
                });
            }
        }
        for ce in &self.player_company.contracted_engines {
            let source = EngineSource::Contracted(ce.id);
            for (fi, flaw) in ce.flaws.iter().enumerate() {
                flaw_table.push(FlawRef {
                    engine_id: ce.design.id,
                    engine_name: ce.design.name.clone(),
                    activation_chance: flaw.activation_chance,
                    consequence: flaw.consequence.clone(),
                    description: flaw.description.clone(),
                    source,
                    flaw_index: fi,
                });
            }
        }

        // Snapshot rocket project PerDay flaws for endurance checking.
        struct RocketFlawRef {
            project_id: RocketProjectId,
            flaw_index: usize,
            daily_rate: f64,
            consequence: FlawConsequence,
            description: String,
        }
        let mut rocket_flaw_table: Vec<RocketFlawRef> = Vec::new();
        for rp in &self.player_company.rocket_projects {
            for (fi, flaw) in rp.flaws.iter().enumerate() {
                if flaw.trigger == FlawTrigger::PerDay {
                    rocket_flaw_table.push(RocketFlawRef {
                        project_id: rp.project_id,
                        flaw_index: fi,
                        daily_rate: flaw.daily_rate(),
                        consequence: flaw.consequence.clone(),
                        description: flaw.description.clone(),
                    });
                }
            }
        }

        // Snapshot reactor flaws keyed by reactor design id. PerFlight
        // flaws roll when a reactor's stage group fires; PerDay endurance
        // flaws roll each day in transit.
        struct ReactorFlawRef {
            reactor_id: crate::reactor::ReactorId,
            flaw_index: usize,
            trigger: FlawTrigger,
            activation_chance: f64,
            daily_rate: f64,
            consequence: FlawConsequence,
            description: String,
        }
        let mut reactor_flaw_table: Vec<ReactorFlawRef> = Vec::new();
        for rp in &self.player_company.reactor_projects {
            for (fi, flaw) in rp.flaws.iter().enumerate() {
                reactor_flaw_table.push(ReactorFlawRef {
                    reactor_id: rp.design.id,
                    flaw_index: fi,
                    trigger: flaw.trigger,
                    activation_chance: flaw.activation_chance,
                    daily_rate: flaw.daily_rate(),
                    consequence: flaw.consequence.clone(),
                    description: flaw.description.clone(),
                });
            }
        }

        // Track flaw discoveries to apply after the flight loop
        let mut flaw_discoveries: Vec<(EngineSource, usize, String)> = Vec::new();
        // Track rocket project flaw discoveries (project_id, flaw_index)
        let mut rocket_flaw_discoveries: Vec<(RocketProjectId, usize)> = Vec::new();
        // Track reactor project flaw discoveries (reactor_id, flaw_index)
        let mut reactor_flaw_discoveries: Vec<(crate::reactor::ReactorId, usize)> = Vec::new();

        for (i, flight) in self.active_flights.iter_mut().enumerate() {
            if !matches!(flight.status, FlightStatus::InTransit) {
                continue;
            }

            // Set to the flaw description if a catastrophic StageLoss
            // activates this tick — the vehicle is destroyed (broke apart)
            // rather than merely stranded.
            let mut flight_lost: Option<String> = None;

            if flight.leg_days_remaining > 0 {
                flight.leg_days_remaining -= 1;
            }

            // Power tick: drain or recharge batteries from supply vs.
            // housekeeping demand at the current location's solar
            // distance. Brownout strands the flight (housekeeping lost →
            // loss of control). No-op for grandfathered designs with no
            // explicit power sources.
            let sun_au = crate::location::DELTA_V_MAP
                .location(&flight.current_location)
                .map_or(1.0, |l| l.sun_distance_au());
            let brownout = flight.rocket.run_daily_power_tick(&flight.design, sun_au);
            if brownout {
                flight.status = FlightStatus::Stranded;
                stranded_indices.push(i);
                let evt = GameEvent::PowerLost {
                    rocket_name: flight.rocket_name.clone(),
                    location: crate::contract::destination_display_name(
                        &flight.current_location).to_string(),
                };
                events.push(evt);
                continue;
            }

            // Roll endurance (PerDay) flaws for this flight's rocket project
            for rf in &rocket_flaw_table {
                if rf.project_id != flight.rocket_project_id {
                    continue;
                }
                if self.seed.contingent_rng.gen::<f64>() < rf.daily_rate {
                    // Pick a random attached stage group and stage
                    let attached: Vec<(usize, usize)> = flight.design.stage_groups.iter()
                        .enumerate()
                        .flat_map(|(gi, group)| {
                            let stage_states = &flight.rocket.stage_states;
                            group.iter().enumerate()
                                .filter(move |(si, _)| {
                                    stage_states.get(gi)
                                        .and_then(|g| g.get(*si))
                                        .is_some_and(|ss| ss.attached)
                                })
                                .map(move |(si, _)| (gi, si))
                        })
                        .collect();
                    if attached.is_empty() {
                        continue;
                    }
                    let (gi, si) = attached[self.seed.contingent_rng.gen_range(0..attached.len())];

                    crate::launch::apply_consequence_to_stage(
                        &mut flight.design,
                        &rf.consequence,
                        gi, si,
                    );
                    if matches!(rf.consequence, FlawConsequence::StageLoss) {
                        flight_lost = Some(rf.description.clone());
                    }

                    let evt = GameEvent::MidFlightFlawActivated {
                        rocket_name: flight.rocket_name.clone(),
                        flaw_description: rf.description.clone(),
                        consequence: rf.consequence.to_string(),
                    };
                    events.push(evt);

                    rocket_flaw_discoveries.push((rf.project_id, rf.flaw_index));
                }
            }

            // Roll reactor flaws for reactors on attached stages. A
            // reactor runs from flight start, so its one-shot PerFlight
            // flaws roll once — on the flight's first in-transit tick —
            // while PerDay endurance flaws roll every day. Each installed
            // reactor rolls independently.
            if !reactor_flaw_table.is_empty() {
                let roll_perflight = !flight.reactor_flaws_rolled;
                let mut reactor_instances: Vec<(usize, usize, crate::reactor::ReactorId)> = Vec::new();
                for (gi, group) in flight.design.stage_groups.iter().enumerate() {
                    for (si, stage) in group.iter().enumerate() {
                        let attached = flight.rocket.stage_states.get(gi)
                            .and_then(|g| g.get(si))
                            .is_some_and(|ss| ss.attached);
                        if !attached {
                            continue;
                        }
                        for src in &stage.power_sources {
                            if let crate::power::PowerSourceKind::Reactor { design: rd } = &src.kind {
                                reactor_instances.push((gi, si, rd.id));
                            }
                        }
                    }
                }
                for (gi, si, reactor_id) in reactor_instances {
                    for rf in &reactor_flaw_table {
                        if rf.reactor_id != reactor_id {
                            continue;
                        }
                        let fires = match rf.trigger {
                            FlawTrigger::PerDay =>
                                self.seed.contingent_rng.gen::<f64>() < rf.daily_rate,
                            FlawTrigger::PerFlight =>
                                roll_perflight
                                    && self.seed.contingent_rng.gen::<f64>() < rf.activation_chance,
                        };
                        if fires {
                            crate::launch::apply_reactor_consequence_to_stage(
                                &mut flight.design,
                                &rf.consequence,
                                gi, si, reactor_id,
                            );
                            if matches!(rf.consequence, FlawConsequence::StageLoss) {
                                flight_lost = Some(rf.description.clone());
                            }
                            let evt = GameEvent::MidFlightFlawActivated {
                                rocket_name: flight.rocket_name.clone(),
                                flaw_description: rf.description.clone(),
                                consequence: rf.consequence.to_string(),
                            };
                            events.push(evt);
                            reactor_flaw_discoveries.push((reactor_id, rf.flaw_index));
                        }
                    }
                }
                flight.reactor_flaws_rolled = true;
            }

            // A catastrophic stage loss during the daily rolls destroys
            // the vehicle — fail it now rather than letting the downstream
            // dv check report it as merely stranded.
            if let Some(reason) = flight_lost.take() {
                flight.status = FlightStatus::Failed { reason };
                lost_indices.push(i);
                continue;
            }

            if flight.leg_days_remaining == 0 {
                // Leg complete — consume propellant for this leg
                if let Some(leg) = flight.route.get(flight.current_leg) {
                    let dv_cost = leg.delta_v_cost;
                    let ambient = leg.ambient_pressure_pa;
                    let burn_result = flight.rocket.burn_sequential(&flight.design, dv_cost, ambient);

                    flight.current_location = leg.to.clone();
                    flight.rocket.location = leg.to.clone();

                    // Check overexpansion destruction for atmospheric legs.
                    // Only the first burned group is at sea level; upper groups
                    // fire at high altitude. Also skip groups already checked at launch.
                    if ambient > 0.0 {
                        let first_burned = burn_result.groups_burned.first().copied();
                        for &gi in &burn_result.groups_burned {
                            // Only the first burned group faces atmospheric pressure
                            if Some(gi) != first_burned {
                                continue;
                            }
                            if flight.flaw_rolled_groups.contains(&gi) {
                                continue; // already checked during launch sim
                            }
                            if let Some(group) = flight.design.stage_groups.get_mut(gi) {
                                for stage in group.iter_mut() {
                                    let risk = stage.engine.overexpansion_destruction_risk(ambient);
                                    if risk <= 0.0 { continue; }
                                    let mut engines_lost = 0u32;
                                    for _ in 0..stage.engine_count {
                                        if self.seed.contingent_rng.gen::<f64>() < risk {
                                            engines_lost += 1;
                                        }
                                    }
                                    if engines_lost > 0 {
                                        if engines_lost >= stage.engine_count {
                                            stage.engine_count = 0;
                                            stage.engine.thrust_n = 0.0;
                                            stage.engine.isp_s = 0.0;
                                            stage.propellant_mass_kg = 0.0;
                                        } else {
                                            stage.engine_count -= engines_lost;
                                        }
                                        let evt = GameEvent::MidFlightFlawActivated {
                                            rocket_name: flight.rocket_name.clone(),
                                            flaw_description: format!(
                                                "{} engine(s) destroyed by flow separation",
                                                engines_lost,
                                            ),
                                            consequence: "Engine destruction".to_string(),
                                        };
                                        events.push(evt);
                                    }
                                }
                            }
                        }
                    }

                    // Roll mid-flight flaws for groups that burned propellant
                    // (must happen before stranding check — stage was used even if burn fell short)
                    // Filter to groups not yet rolled for flaws
                    let new_burned: Vec<usize> = burn_result.groups_burned.iter()
                        .copied()
                        .filter(|gi| !flight.flaw_rolled_groups.contains(gi))
                        .collect();
                    if !new_burned.is_empty() {
                        for &gi in &new_burned {
                            flight.flaw_rolled_groups.insert(gi);
                        }
                        // Collect (group_index, stage_index, engine_id, engine_count) from newly-burned stages
                        let mut burned_stages: Vec<(usize, usize, EngineId, u32)> = Vec::new();
                        for &gi in &new_burned {
                            if let Some(group) = flight.design.stage_groups.get(gi) {
                                for (si, stage) in group.iter().enumerate() {
                                    burned_stages.push((gi, si, stage.engine.id, stage.engine_count));
                                }
                            }
                        }

                        // Roll flaws for each engine used in burned groups
                        for &(gi, si, engine_id, engine_count) in &burned_stages {
                            for flaw_ref in &flaw_table {
                                if flaw_ref.engine_id != engine_id {
                                    continue;
                                }
                                let effective_p = 1.0 - (1.0 - flaw_ref.activation_chance)
                                    .powi(engine_count as i32);
                                if self.seed.contingent_rng.gen::<f64>() < effective_p {
                                    flight.flaws_activated.push(crate::launch::FlawActivation {
                                        flaw_description: flaw_ref.description.clone(),
                                        consequence: flaw_ref.consequence.clone(),
                                        engine_name: flaw_ref.engine_name.clone(),
                                    });

                                    // Apply consequence to the stage that has the flaw
                                    crate::launch::apply_consequence_to_stage(
                                        &mut flight.design,
                                        &flaw_ref.consequence,
                                        gi,
                                        si,
                                    );
                                    if matches!(flaw_ref.consequence, FlawConsequence::StageLoss) {
                                        flight_lost = Some(flaw_ref.description.clone());
                                    }

                                    let evt = GameEvent::MidFlightFlawActivated {
                                        rocket_name: flight.rocket_name.clone(),
                                        flaw_description: flaw_ref.description.clone(),
                                        consequence: flaw_ref.consequence.to_string(),
                                    };
                                    events.push(evt);

                                    flaw_discoveries.push((
                                        flaw_ref.source,
                                        flaw_ref.flaw_index,
                                        flaw_ref.engine_name.clone(),
                                    ));
                                }
                            }
                        }

                        // A stage loss during the burn destroys the vehicle.
                        if let Some(reason) = flight_lost.take() {
                            flight.status = FlightStatus::Failed { reason };
                            lost_indices.push(i);
                            continue;
                        }

                        // After flaw application, recheck remaining dv for stranding
                        let remaining_dv = flight.rocket.remaining_delta_v(&flight.design);
                        let remaining_route_dv: f64 = flight.route.iter()
                            .skip(flight.current_leg + 1)
                            .map(|leg| leg.delta_v_cost)
                            .sum();
                        if remaining_route_dv > 0.0 && remaining_dv < remaining_route_dv * 0.5 {
                            flight.status = FlightStatus::Stranded;
                            stranded_indices.push(i);
                            continue;
                        }
                    }

                    // Check if burn fell significantly short — strand the flight
                    if burn_result.dv_achieved < dv_cost * 0.95 {
                        flight.status = FlightStatus::Stranded;
                        stranded_indices.push(i);
                        continue;
                    }
                }

                // Advance to next leg
                flight.current_leg += 1;
                if flight.current_leg < flight.route.len() {
                    flight.leg_days_remaining = flight.route[flight.current_leg].total_days();
                } else {
                    // All legs complete
                    flight.status = FlightStatus::Arrived;
                    arrived_indices.push(i);
                }
            }
        }

        // Apply flaw discoveries to engine/rocket projects
        for (source, flaw_index, _engine_name) in &flaw_discoveries {
            match source {
                EngineSource::PlayerDesign(project_id) => {
                    if let Some(ep) = self.player_company.engine_projects.iter_mut()
                        .find(|ep| ep.project_id == *project_id)
                    {
                        if *flaw_index < ep.flaws.len() && !ep.flaws[*flaw_index].discovered {
                            ep.flaws[*flaw_index].discovered = true;
                            let evt = GameEvent::FlawDiscovered {
                                engine_name: ep.design.name.clone(),
                                flaw_description: ep.flaws[*flaw_index].description.clone(),
                            };
                            events.push(evt);
                        }
                    }
                }
                EngineSource::Contracted(ce_id) => {
                    if let Some(ce) = self.player_company.contracted_engines.iter_mut()
                        .find(|ce| ce.id == *ce_id)
                    {
                        if *flaw_index < ce.flaws.len() {
                            ce.flaws[*flaw_index].discovered = true;
                        }
                    }
                }
            }
        }

        // Apply rocket project endurance flaw discoveries
        for (project_id, flaw_index) in &rocket_flaw_discoveries {
            if let Some(rp) = self.player_company.rocket_projects.iter_mut()
                .find(|rp| rp.project_id == *project_id)
            {
                if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                    rp.flaws[*flaw_index].discovered = true;
                    let evt = GameEvent::FlawDiscovered {
                        engine_name: rp.design.name.clone(),
                        flaw_description: rp.flaws[*flaw_index].description.clone(),
                    };
                    events.push(evt);
                }
            }
        }

        // Apply reactor project flaw discoveries (keyed by reactor id).
        for (reactor_id, flaw_index) in &reactor_flaw_discoveries {
            if let Some(rp) = self.player_company.reactor_projects.iter_mut()
                .find(|rp| rp.design.id == *reactor_id)
            {
                if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                    rp.flaws[*flaw_index].discovered = true;
                    let evt = GameEvent::ReactorFlawDiscovered {
                        reactor_name: rp.design.name.clone(),
                        flaw_description: rp.flaws[*flaw_index].description.clone(),
                    };
                    events.push(evt);
                }
            }
        }

        // Resolve arrived / stranded / lost flights. Process in reverse
        // index order so removals don't shift the indices still to remove.
        enum FlightEnd { Arrived, Stranded, Lost }
        let mut remove_indices: Vec<(usize, FlightEnd)> = Vec::new();
        for &i in &arrived_indices {
            remove_indices.push((i, FlightEnd::Arrived));
        }
        for &i in &stranded_indices {
            remove_indices.push((i, FlightEnd::Stranded));
        }
        for &i in &lost_indices {
            remove_indices.push((i, FlightEnd::Lost));
        }
        remove_indices.sort_by_key(|&(i, _)| std::cmp::Reverse(i));

        for (i, end) in remove_indices {
            let flight = self.active_flights.remove(i);
            let location = crate::contract::destination_display_name(&flight.current_location)
                .to_string();
            match end {
                FlightEnd::Arrived => {
                    let arrival_events = self.resolve_arrived_flight(flight);
                    events.extend(arrival_events);
                }
                FlightEnd::Stranded => {
                    let evt = GameEvent::SpacecraftStranded {
                        rocket_name: flight.rocket_name.clone(),
                        location,
                    };
                    events.push(evt);
                }
                FlightEnd::Lost => {
                    // Vehicle destroyed mid-flight — the mission (and any
                    // payload) is a total loss, and it dents reputation
                    // like a launch failure.
                    let reason = match &flight.status {
                        FlightStatus::Failed { reason } => reason.clone(),
                        _ => "stage loss".to_string(),
                    };
                    let manifest: Vec<crate::contract::ContractId> = flight.payloads.iter()
                        .filter_map(|p| match p {
                            Payload::ContractDelivery { contract_id, .. } => Some(*contract_id),
                            _ => None,
                        })
                        .collect();
                    let severity = self.manifest_failure_severity(&manifest);
                    self.player_company.reputation.on_launch_failure(&self.balance.reputation, severity);
                    let evt = GameEvent::SpacecraftLost {
                        rocket_name: flight.rocket_name.clone(),
                        location,
                        reason,
                    };
                    events.push(evt);
                }
            }
        }

        events
    }

    /// Resolve a flight that has arrived at its destination.
    pub(super) fn resolve_arrived_flight(&mut self, flight: Flight) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let destination = flight.destination().to_string();
        let dest_display = crate::contract::destination_display_name(&destination);
        let total_payload_kg = flight.total_payload_kg();

        let evt = GameEvent::FlightArrived {
            rocket_name: flight.rocket_name.clone(),
            destination: dest_display.to_string(),
        };
        events.push(evt);

        // Determine outcome based on launch sim result (stored in flight)
        let is_partial = flight.launch_partial;

        if is_partial {
            let manifest: Vec<crate::contract::ContractId> = flight.payloads.iter()
                .filter_map(|p| match p {
                    Payload::ContractDelivery { contract_id, .. } => Some(*contract_id),
                    _ => None,
                })
                .collect();
            let severity = self.manifest_failure_severity(&manifest);
            self.player_company.reputation.on_launch_partial_failure(
                &self.balance.reputation, severity,
            );
        } else {
            self.player_company.reputation.on_launch_success(&self.balance.reputation);
        }

        // Process each payload. Spacecraft payloads marked for this
        // destination are detached and pushed into the fleet; others
        // (contracts/test masses) are completed/discarded as before.
        let mut contract_id_for_record = None;
        let mut deployed_spacecraft: Vec<Payload> = Vec::new();
        let mut remaining_payloads: Vec<Payload> = Vec::new();
        for payload in flight.payloads {
            match payload {
                Payload::ContractDelivery { contract_id, .. } => {
                    contract_id_for_record = Some(contract_id);

                    if let Some(ci) = self.player_company.active_contracts.iter()
                        .position(|c| c.id == contract_id)
                    {
                        let contract = &self.player_company.active_contracts[ci];
                        let payment = if is_partial {
                            contract.payment * 0.5
                        } else {
                            contract.payment
                        };
                        let contract_name = contract.name.clone();
                        self.player_company.money += payment;
                        self.record_income(payment);
                        self.player_company.reputation.on_contract_launch(&self.balance.reputation);

                        let pay_evt = GameEvent::PaymentReceived {
                            amount: payment,
                            contract_name,
                        };
                        events.push(pay_evt);

                        self.player_company.active_contracts.remove(ci);
                    }
                }
                Payload::TestMass { .. } => {
                    // No payment for test launches.
                }
                Payload::Spacecraft { deploy_at: Some(ref d), .. } if *d == destination => {
                    deployed_spacecraft.push(payload);
                }
                other => {
                    // Spacecraft payload bound for some other waypoint —
                    // not implemented yet (Phase 2). For now keep it on the
                    // arriving rocket as if the carrier were continuing.
                    remaining_payloads.push(other);
                }
            }
        }

        // Generate outcome event
        let outcome = if is_partial {
            let reason = flight.flaws_activated.first()
                .map(|f| f.flaw_description.clone())
                .unwrap_or_else(|| "degraded performance".to_string());
            let evt = GameEvent::LaunchPartialFailure {
                rocket_name: flight.rocket_name.clone(),
                reason: reason.clone(),
            };
            events.push(evt);
            LaunchOutcome::PartialFailure { reason }
        } else {
            let evt = GameEvent::LaunchSuccess {
                rocket_name: flight.rocket_name.clone(),
                destination: dest_display.to_string(),
            };
            events.push(evt);
            LaunchOutcome::Success
        };

        // Persist as spacecraft if requested
        let persist = flight.persist;
        let rocket_instance = flight.rocket;
        let design_clone = flight.design;
        let rocket_name = flight.rocket_name;
        let dest_for_spacecraft = destination.clone();

        let record = LaunchRecord {
            launch_date: flight.launch_date,
            rocket_name: rocket_name.clone(),
            contract_id: contract_id_for_record,
            destination: destination.clone(),
            payload_kg: total_payload_kg,
            outcome,
            flaws_activated: flight.flaws_activated,
        };
        self.player_company.launch_history.push(record);

        if persist {
            let sc_id = SpacecraftId(self.next_rocket_id);
            self.next_rocket_id += 1;
            self.spacecraft.push(Spacecraft {
                id: sc_id,
                name: rocket_name,
                rocket: rocket_instance,
                design: design_clone,
                location: dest_for_spacecraft,
                rocket_project_id: flight.rocket_project_id,
                payloads: remaining_payloads,
            });
        }

        // Detach Spacecraft payloads at this destination into the fleet.
        for payload in deployed_spacecraft {
            if let Payload::Spacecraft {
                design, rocket, nested_payloads, rocket_project_id, name, ..
            } = payload {
                let sc_id = SpacecraftId(self.next_rocket_id);
                self.next_rocket_id += 1;
                let evt = GameEvent::SpacecraftDeployed {
                    spacecraft_name: name.clone(),
                    location: dest_display.to_string(),
                };
                events.push(evt);
                self.spacecraft.push(Spacecraft {
                    id: sc_id,
                    name,
                    rocket,
                    design,
                    location: destination.clone(),
                    rocket_project_id,
                    payloads: nested_payloads,
                });
            }
        }

        events
    }

    /// Send a spacecraft on a new flight to a destination. Any payloads
    /// the spacecraft is still carrying ride along; those whose `deploy_at`
    /// matches the destination will be detached on arrival (via the regular
    /// arrival path).
    pub fn fly_spacecraft(&mut self, spacecraft_index: usize, destination: &str) {
        if spacecraft_index >= self.spacecraft.len() {
            return;
        }
        let mut sc = self.spacecraft.remove(spacecraft_index);
        // Recompute payload mass from current carried payloads (live value
        // may differ from rocket.payload_mass_kg if payloads were detached
        // earlier). Sync the rocket's cached payload mass too so dv math
        // stays correct.
        let payload_mass: f64 = sc.payloads.iter().map(|p| p.mass_kg()).sum();
        sc.rocket.payload_mass_kg = payload_mass;

        // Refuse the flight if the active group's electric engines
        // can't produce thrust at the spacecraft's current location.
        let sun_au_at_takeoff = crate::location::DELTA_V_MAP
            .location(&sc.location)
            .map_or(1.0, |l| l.sun_distance_au());
        let avail_power = sc.design.power_for_engines_w(sun_au_at_takeoff);
        let first_group_thrust = sc.design
            .group_effective_thrust_n(0, avail_power);
        if first_group_thrust <= 0.0 {
            self.spacecraft.insert(spacecraft_index, sc);
            return;
        }

        let path = crate::location::DELTA_V_MAP
            .shortest_path_for_rocket(
                &sc.location, destination, &sc.design, payload_mass,
            );
        let route = match path {
            Some((path, _)) => crate::flight::build_route_for_rocket(
                &path, &sc.design, &sc.rocket, payload_mass,
            ),
            None => {
                // No valid path — put the spacecraft back and abort
                self.spacecraft.insert(spacecraft_index, sc);
                return;
            }
        };
        if route.is_empty() {
            self.spacecraft.insert(spacecraft_index, sc);
            return;
        }

        let flight_id = FlightId(self.next_flight_id);
        self.next_flight_id += 1;

        let leg_days = route.first().map(|l| l.total_days()).unwrap_or(0);
        let dest_display = crate::contract::destination_display_name(destination);

        let flight = Flight {
            id: flight_id,
            // Spacecraft ops are player-only today.
            company: crate::flight::CompanyRef::Player,
            rocket_name: sc.name.clone(),
            rocket_project_id: crate::rocket_project::RocketProjectId(0), // no project for spacecraft flights
            design: sc.design,
            rocket: sc.rocket,
            payloads: sc.payloads,
            current_location: sc.location,
            route,
            current_leg: 0,
            leg_days_remaining: leg_days,
            status: FlightStatus::InTransit,
            flaws_activated: vec![],
            launch_date: self.date,
            persist: true, // spacecraft flights always persist
            launch_partial: false,
            flaw_rolled_groups: std::collections::HashSet::new(),
            reactor_flaws_rolled: false,
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: sc.name,
            destination: dest_display.to_string(),
        };
        self.event_log.push(self.date, evt);
    }

    /// Dock spacecraft `small_idx` onto `large_idx`. Both must be at the
    /// same location and refer to different spacecraft. The smaller is
    /// removed from `game.spacecraft` and re-wrapped as a
    /// `Payload::Spacecraft` (with `deploy_at = None`, meaning manual
    /// undock only) on the larger. Returns true on success.
    pub fn dock_spacecraft(&mut self, small_idx: usize, large_idx: usize) -> bool {
        if small_idx == large_idx { return false; }
        let n = self.spacecraft.len();
        if small_idx >= n || large_idx >= n { return false; }
        if self.spacecraft[small_idx].location != self.spacecraft[large_idx].location {
            return false;
        }
        // Remove the smaller first; if its index was below the larger's,
        // the larger's index has shifted down by one.
        let small = self.spacecraft.remove(small_idx);
        let adjusted_large = if small_idx < large_idx { large_idx - 1 } else { large_idx };
        let location = small.location.clone();
        let small_name = small.name.clone();
        let large_name = self.spacecraft[adjusted_large].name.clone();

        let payload = crate::flight::Payload::Spacecraft {
            deploy_at: None,
            design: small.design,
            rocket: small.rocket,
            nested_payloads: small.payloads,
            rocket_project_id: small.rocket_project_id,
            name: small.name,
        };
        self.spacecraft[adjusted_large].payloads.push(payload);

        let evt = GameEvent::SpacecraftDocked {
            small: small_name,
            large: large_name,
            location: crate::contract::destination_display_name(&location).to_string(),
        };
        self.event_log.push(self.date, evt);
        true
    }

    /// Undock the `payload_idx`-th payload of `carrier_idx` and add it to
    /// the fleet at the carrier's location. The payload must be a
    /// `Payload::Spacecraft`. Returns true on success.
    pub fn undock_payload(&mut self, carrier_idx: usize, payload_idx: usize) -> bool {
        if carrier_idx >= self.spacecraft.len() { return false; }
        if payload_idx >= self.spacecraft[carrier_idx].payloads.len() { return false; }
        let is_spacecraft = matches!(
            self.spacecraft[carrier_idx].payloads[payload_idx],
            crate::flight::Payload::Spacecraft { .. },
        );
        if !is_spacecraft { return false; }

        let location = self.spacecraft[carrier_idx].location.clone();
        let carrier_name = self.spacecraft[carrier_idx].name.clone();
        let payload = self.spacecraft[carrier_idx].payloads.remove(payload_idx);
        let crate::flight::Payload::Spacecraft {
            design, rocket, nested_payloads, rocket_project_id, name, ..
        } = payload else {
            return false; // unreachable given the matches! above
        };
        let payload_name = name.clone();

        let sc_id = SpacecraftId(self.next_rocket_id);
        self.next_rocket_id += 1;
        self.spacecraft.push(Spacecraft {
            id: sc_id, name, rocket, design,
            location: location.clone(),
            rocket_project_id,
            payloads: nested_payloads,
        });

        let evt = GameEvent::SpacecraftUndocked {
            payload: payload_name,
            carrier: carrier_name,
            location: crate::contract::destination_display_name(&location).to_string(),
        };
        self.event_log.push(self.date, evt);
        true
    }
}
