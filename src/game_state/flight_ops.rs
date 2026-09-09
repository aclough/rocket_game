//! Launch and flight operations: manifest assembly, launches,
//! in-transit flight advancement, and parked-spacecraft ops
//! (fly / dock / undock).


use crate::engine_project::EngineSource;
use crate::flight::{Flight, FlightId, FlightStatus, Payload};
use crate::event::{GameEvent, ProjectEvent};
use crate::project::ProjectKind;
use crate::launch::{self, FlawOwner, FlawRoll, FlawTables, LaunchRecord, LaunchOutcome};
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

        // Mark what fired as discovered on the projects that own it.
        let discoveries = FlawDiscoveries {
            engines: sim.engine_discoveries.clone(),
            rockets: sim.rocket_flaw_discoveries.iter()
                .map(|&fi| (inv_rocket.rocket_project_id, fi))
                .collect(),
            reactors: Vec::new(),
        };
        let mut discovered = Vec::new();
        self.apply_flaw_discoveries(discoveries, &mut discovered);
        self.emit_all(&mut events, discovered);


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
            self.emit(&mut events, evt);

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
            launch_partial: match &sim.outcome {
                LaunchOutcome::PartialFailure { reason } => Some(reason.clone()),
                _ => None,
            },
            flaw_rolled_groups: sim.flaw_rolled_groups,
            reactor_flaws_rolled: false,
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: inv_rocket.rocket_name,
            destination: dest_display.to_string(),
        };
        self.emit(&mut events, evt);

        self.speed = GameSpeed::Paused;

        Some((events, None))
    }

    /// Process daily flight advancement. Returns events generated.
    pub(super) fn advance_flights(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let tables = FlawTables::snapshot(&self.player_company);
        let mut discoveries = FlawDiscoveries::default();
        let mut ended: Vec<(usize, FlightEnd)> = Vec::new();

        for (i, flight) in self.active_flights.iter_mut().enumerate() {
            if !matches!(flight.status, FlightStatus::InTransit) {
                continue;
            }
            let end = tick_flight(
                flight, &tables, &mut self.seed.contingent_rng, &mut events, &mut discoveries,
            );
            if let Some(end) = end {
                ended.push((i, end));
            }
        }

        self.apply_flaw_discoveries(discoveries, &mut events);

        // Resolve arrived / stranded / lost flights. Process in reverse
        // index order so removals don't shift the indices still to remove.
        ended.sort_by_key(|&(i, _)| std::cmp::Reverse(i));
        for (i, end) in ended {
            let flight = self.active_flights.remove(i);
            let location = crate::contract::destination_display_name(&flight.current_location)
                .to_string();
            match end {
                FlightEnd::Arrived => {
                    let arrival_events = self.resolve_arrived_flight(flight);
                    events.extend(arrival_events);
                }
                FlightEnd::Stranded => {
                    events.push(GameEvent::SpacecraftStranded {
                        rocket_name: flight.rocket_name.clone(),
                        location,
                    });
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
                    events.push(GameEvent::SpacecraftLost {
                        rocket_name: flight.rocket_name.clone(),
                        location,
                        reason,
                    });
                }
            }
        }

        events
    }

    /// Mark the flaws that fired today as discovered on the projects
    /// that own them, with a discovery event for each newly revealed
    /// one. Contracted engines' flaws are marked but not announced —
    /// they are someone else's design.
    fn apply_flaw_discoveries(&mut self, discoveries: FlawDiscoveries, events: &mut Vec<GameEvent>) {
        let mut news = Vec::new();
        for (owner, flaw_index) in &discoveries.engines {
            match owner {
                FlawOwner::Engine { source: EngineSource::PlayerDesign(project_id), .. } => {
                    if let Some(ep) = self.player_company.engine_projects.iter_mut()
                        .find(|ep| ep.project_id == *project_id)
                    {
                        if *flaw_index < ep.flaws.len() && !ep.flaws[*flaw_index].discovered {
                            ep.flaws[*flaw_index].discovered = true;
                            news.push(GameEvent::project(
                                ProjectKind::Engine, ep.design.name.clone(),
                                ProjectEvent::FlawDiscovered {
                                    description: ep.flaws[*flaw_index].description.clone(),
                                },
                            ));
                        }
                    }
                }
                FlawOwner::Engine { source: EngineSource::Contracted(ce_id), .. } => {
                    if let Some(ce) = self.player_company.contracted_engines.iter_mut()
                        .find(|ce| ce.id == *ce_id)
                    {
                        if *flaw_index < ce.flaws.len() {
                            ce.flaws[*flaw_index].discovered = true;
                        }
                    }
                }
                FlawOwner::Rocket(_) | FlawOwner::Reactor(_) => {}
            }
        }
        for (project_id, flaw_index) in &discoveries.rockets {
            if let Some(rp) = self.player_company.rocket_projects.iter_mut()
                .find(|rp| rp.project_id == *project_id)
            {
                if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                    rp.flaws[*flaw_index].discovered = true;
                    news.push(GameEvent::project(
                        ProjectKind::Rocket, rp.design.name.clone(),
                        ProjectEvent::FlawDiscovered {
                            description: rp.flaws[*flaw_index].description.clone(),
                        },
                    ));
                }
            }
        }
        for (reactor_id, flaw_index) in &discoveries.reactors {
            if let Some(rp) = self.player_company.reactor_projects.iter_mut()
                .find(|rp| rp.design.id == *reactor_id)
            {
                if *flaw_index < rp.flaws.len() && !rp.flaws[*flaw_index].discovered {
                    rp.flaws[*flaw_index].discovered = true;
                    news.push(GameEvent::project(
                        ProjectKind::Reactor, rp.design.name.clone(),
                        ProjectEvent::FlawDiscovered {
                            description: rp.flaws[*flaw_index].description.clone(),
                        },
                    ));
                }
            }
        }
        events.extend(news);
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
        let partial_reason = flight.launch_partial.clone();
        let is_partial = partial_reason.is_some();

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
                        self.player_company.credit(payment);
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
        let outcome = if let Some(reason) = partial_reason {
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
            launch_partial: None,
            flaw_rolled_groups: std::collections::HashSet::new(),
            reactor_flaws_rolled: false,
        };

        self.active_flights.push(flight);

        let evt = GameEvent::FlightDeparted {
            rocket_name: sc.name,
            destination: dest_display.to_string(),
        };
        self.log(evt);
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
        self.log(evt);
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
        self.log(evt);
        true
    }
}

// ── Parked spacecraft ────────────────────────────────────────────────

impl GameState {
    /// The daily tick for spacecraft sitting at a location: the same
    /// power balance flights run — brownout kills the craft (loss of
    /// attitude/comms, the same lethal outcome as a flight stranding),
    /// and anything aboard is lost with it — then the endurance flaws of
    /// its rocket design roll, as they do in transit.
    ///
    /// No "had charge before" guard on the brownout: a fuel-cell-only
    /// craft never has battery charge yet still dies on the day its
    /// propellant runs out, and removing-on-brownout is self-debouncing
    /// (the spacecraft is gone after one event).
    pub(super) fn tick_parked_spacecraft(&mut self, events: &mut Vec<GameEvent>) {
        let mut browned_out: Vec<usize> = Vec::new();
        for (i, sc) in self.spacecraft.iter_mut().enumerate() {
            let sun_au = crate::location::DELTA_V_MAP
                .location(&sc.location)
                .map_or(1.0, |l| l.sun_distance_au());
            if sc.rocket.run_daily_power_tick(&sc.design, sun_au) {
                browned_out.push(i);
            }
        }
        for &i in browned_out.iter().rev() {
            let sc = self.spacecraft.remove(i);
            let evt = GameEvent::PowerLost {
                rocket_name: sc.name,
                location: crate::contract::destination_display_name(&sc.location)
                    .to_string(),
            };
            self.emit(events, evt);
        }

        // Endurance flaws age a parked vehicle the same way they age a
        // flight. A StageLoss disables the stage but the parked craft
        // stays — there is no mission to lose.
        let tables = FlawTables::snapshot(&self.player_company);
        let mut discoveries = FlawDiscoveries::default();
        let mut news = Vec::new();
        for sc in &mut self.spacecraft {
            let roll = crate::launch::roll_endurance_flaws(
                &mut self.seed.contingent_rng, &mut sc.design, &sc.rocket,
                sc.rocket_project_id, &tables.rockets,
            );
            for activation in &roll.activations {
                news.push(GameEvent::MidFlightFlawActivated {
                    rocket_name: sc.name.clone(),
                    flaw_description: activation.flaw_description.clone(),
                    consequence: activation.consequence.to_string(),
                });
            }
            discoveries.record(&roll);
        }
        self.emit_all(events, news);
        // Parked craft don't announce discoveries (they never did); the
        // project just learns the flaw is real.
        let mut quiet = Vec::new();
        self.apply_flaw_discoveries(discoveries, &mut quiet);
    }
}

// ── One flight, one day ──────────────────────────────────────────────

/// Why a flight left the active list this tick.
enum FlightEnd {
    Arrived,
    Stranded,
    Lost,
}

/// Flaws that fired today, to be marked discovered on their owners once
/// the vehicles they were snapshotted for have all been ticked.
#[derive(Default)]
struct FlawDiscoveries {
    engines: Vec<(FlawOwner, usize)>,
    rockets: Vec<(crate::rocket_project::RocketProjectId, usize)>,
    reactors: Vec<(crate::reactor::ReactorId, usize)>,
}

impl FlawDiscoveries {
    fn record(&mut self, roll: &FlawRoll) {
        for &(owner, fi) in &roll.discoveries {
            match owner {
                FlawOwner::Engine { .. } => self.engines.push((owner, fi)),
                FlawOwner::Rocket(id) => self.rockets.push((id, fi)),
                FlawOwner::Reactor(id) => self.reactors.push((id, fi)),
            }
        }
    }
}

/// The news for each flaw a roll set off.
fn push_activation_events(events: &mut Vec<GameEvent>, rocket_name: &str, roll: &FlawRoll) {
    for a in &roll.activations {
        events.push(GameEvent::MidFlightFlawActivated {
            rocket_name: rocket_name.to_string(),
            flaw_description: a.flaw_description.clone(),
            consequence: a.consequence.to_string(),
        });
    }
}

/// Roll the flaws of every reactor on an attached stage. A reactor runs
/// from flight start, so its one-shot `PerFlight` flaws roll once — on
/// the flight's first in-transit tick — while `PerDay` endurance flaws
/// roll every day. Each installed reactor rolls independently; a
/// `StageLoss` stops everything.
fn roll_reactor_flaws(
    rng: &mut rand::rngs::StdRng,
    design: &mut crate::rocket::RocketDesign,
    rocket: &crate::rocket::Rocket,
    roll_perflight: bool,
    table: &[crate::launch::FlawRef],
) -> FlawRoll {
    use rand::Rng;
    use crate::flaw::{FlawConsequence, FlawTrigger};

    let mut roll = FlawRoll::default();
    let mut instances: Vec<(usize, usize, crate::reactor::ReactorId)> = Vec::new();
    for (gi, group) in design.stage_groups.iter().enumerate() {
        for (si, stage) in group.iter().enumerate() {
            let attached = rocket.stage_states.get(gi)
                .and_then(|g| g.get(si))
                .is_some_and(|ss| ss.attached);
            if !attached {
                continue;
            }
            for src in &stage.power_sources {
                if let crate::power::PowerSourceKind::Reactor { design: rd } = &src.kind {
                    instances.push((gi, si, rd.id));
                }
            }
        }
    }
    'reactors: for (gi, si, reactor_id) in instances {
        for rf in table.iter().filter(|f| f.owner == FlawOwner::Reactor(reactor_id)) {
            if roll.lost.is_some() {
                break 'reactors;
            }
            let fires = match rf.trigger {
                FlawTrigger::PerDay => rng.gen::<f64>() < rf.daily_rate,
                FlawTrigger::PerFlight =>
                    roll_perflight && rng.gen::<f64>() < rf.activation_chance,
            };
            if fires {
                launch::apply_reactor_consequence_to_stage(
                    design, &rf.consequence, gi, si, reactor_id,
                );
                roll.activations.push(launch::FlawActivation {
                    flaw_description: rf.description.clone(),
                    consequence: rf.consequence.clone(),
                    engine_name: String::new(),
                });
                roll.discoveries.push((rf.owner, rf.flaw_index));
                if matches!(rf.consequence, FlawConsequence::StageLoss) {
                    roll.lost = Some(rf.description.clone());
                }
            }
        }
    }
    roll
}

/// One day for one flight in transit: the vehicle ages (endurance and
/// reactor flaws), a leg that completes today is flown (burn, then the
/// flaws and flow separation of the stages that just fired, then the
/// stranding checks), and the batteries are balanced at wherever it
/// now is. Events go to `events` unlogged — the tick's caller logs
/// them — and discoveries are applied after every flight has been
/// ticked, since they touch the company the tables were snapshotted
/// from.
fn tick_flight(
    flight: &mut Flight,
    tables: &FlawTables,
    rng: &mut rand::rngs::StdRng,
    events: &mut Vec<GameEvent>,
    discoveries: &mut FlawDiscoveries,
) -> Option<FlightEnd> {
    if flight.leg_days_remaining > 0 {
        flight.leg_days_remaining -= 1;
    }

    // Endurance flaws of the rocket design.
    let roll = launch::roll_endurance_flaws(
        rng, &mut flight.design, &flight.rocket, flight.rocket_project_id, &tables.rockets,
    );
    push_activation_events(events, &flight.rocket_name, &roll);
    discoveries.record(&roll);
    let mut flight_lost = roll.lost;

    if !tables.reactors.is_empty() {
        if flight_lost.is_none() {
            let roll = roll_reactor_flaws(
                rng, &mut flight.design, &flight.rocket,
                !flight.reactor_flaws_rolled, &tables.reactors,
            );
            push_activation_events(events, &flight.rocket_name, &roll);
            discoveries.record(&roll);
            flight_lost = roll.lost;
        }
        flight.reactor_flaws_rolled = true;
    }

    // A catastrophic stage loss during the daily rolls destroys the
    // vehicle — fail it now rather than letting the downstream dv check
    // report it as merely stranded.
    if let Some(reason) = flight_lost.take() {
        flight.status = FlightStatus::Failed { reason };
        return Some(FlightEnd::Lost);
    }

    if flight.leg_days_remaining == 0 {
        let leg = flight.route.get(flight.current_leg)
            .map(|l| (l.delta_v_cost, l.from.clone(), l.to.clone()));
        if let Some((dv_cost, from, to)) = leg {
            // Leg complete — consume propellant for this leg.
            let burn_result = flight.rocket.burn_sequential(&flight.design, dv_cost, &from);
            flight.current_location = to.clone();
            flight.rocket.location = to;

            // Flow separation on an atmospheric leg, at pad pressure: only
            // the first burned group faces it; upper groups fire at
            // altitude, and a group already checked on the pad isn't
            // checked again.
            let ambient = crate::location::DELTA_V_MAP.surface_properties(&from)
                .filter(|p| p.has_atmosphere)
                .map_or(0.0, |p| p.ambient_pressure_pa);
            if ambient > 0.0 {
                if let Some(&gi) = burn_result.groups_burned.first() {
                    if !flight.flaw_rolled_groups.contains(&gi) {
                        if let Some(group) = flight.design.stage_groups.get_mut(gi) {
                            for stage in group.iter_mut() {
                                if let Some(loss) = launch::roll_overexpansion(rng, stage, ambient) {
                                    events.push(GameEvent::MidFlightFlawActivated {
                                        rocket_name: flight.rocket_name.clone(),
                                        flaw_description: format!(
                                            "{} engine(s) destroyed by flow separation",
                                            loss.engines_lost,
                                        ),
                                        consequence: "Engine destruction".to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // Engine flaws for the groups that fired for the first time
            // (before the stranding check — the stage was used even if the
            // burn fell short).
            let new_burned: Vec<usize> = burn_result.groups_burned.iter()
                .copied()
                .filter(|gi| !flight.flaw_rolled_groups.contains(gi))
                .collect();
            if !new_burned.is_empty() {
                flight.flaw_rolled_groups.extend(new_burned.iter().copied());
                let stages: Vec<(usize, usize)> = new_burned.iter()
                    .flat_map(|&gi| {
                        let n = flight.design.stage_groups.get(gi).map_or(0, |g| g.len());
                        (0..n).map(move |si| (gi, si))
                    })
                    .collect();
                let roll = launch::roll_engine_flaws(rng, &mut flight.design, &stages, &tables.engines);
                flight.flaws_activated.extend(roll.activations.iter().cloned());
                push_activation_events(events, &flight.rocket_name, &roll);
                discoveries.record(&roll);
                if let Some(reason) = roll.lost {
                    flight.status = FlightStatus::Failed { reason };
                    return Some(FlightEnd::Lost);
                }

                // After flaw application, recheck remaining dv for stranding.
                let remaining_dv = flight.rocket.remaining_delta_v(&flight.design);
                let remaining_route_dv: f64 = flight.route.iter()
                    .skip(flight.current_leg + 1)
                    .map(|leg| leg.delta_v_cost)
                    .sum();
                if remaining_route_dv > 0.0 && remaining_dv < remaining_route_dv * 0.5 {
                    flight.status = FlightStatus::Stranded;
                    return Some(FlightEnd::Stranded);
                }
            }

            // A burn that fell significantly short strands the flight.
            if burn_result.dv_achieved < dv_cost * 0.95 {
                flight.status = FlightStatus::Stranded;
                return Some(FlightEnd::Stranded);
            }
        }

        flight.current_leg += 1;
        if flight.current_leg < flight.route.len() {
            flight.leg_days_remaining = flight.route[flight.current_leg].total_days();
        } else {
            // All legs complete. No power tick: the flight is about to
            // become a Spacecraft, and the parked-fleet tick charges it
            // there — after its payload is delivered, and exactly once.
            flight.status = FlightStatus::Arrived;
            return Some(FlightEnd::Arrived);
        }
    }

    // Power tick: drain or recharge batteries from supply vs.
    // housekeeping demand at the current location's solar distance.
    // Brownout strands the flight (housekeeping lost → loss of control).
    // Runs on every design: a stage with a bare power rack still carries
    // its default battery, so nothing is exempt. Runs last, so a leg
    // completed this tick is charged at the location it *reached*.
    let sun_au = crate::location::DELTA_V_MAP
        .location(&flight.current_location)
        .map_or(1.0, |l| l.sun_distance_au());
    if flight.rocket.run_daily_power_tick(&flight.design, sun_au) {
        flight.status = FlightStatus::Stranded;
        events.push(GameEvent::PowerLost {
            rocket_name: flight.rocket_name.clone(),
            location: crate::contract::destination_display_name(&flight.current_location).to_string(),
        });
        return Some(FlightEnd::Stranded);
    }
    None
}
