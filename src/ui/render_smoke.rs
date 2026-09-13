//! Render smoke test (17_4_UI.md step 0): every tab and every reachable
//! modal, drawn over one game with a lot going on, at 80 and 120
//! columns. The simulate oracle never reaches the UI, so this is the
//! gate the UI refactors run behind: a moved helper that panics, or a
//! modal that stops drawing, fails here before anyone plays it.

use ratatui::crossterm::event::KeyCode;

use crate::contract::{Campaign, CampaignId, CampaignStatus};
use crate::flight::Payload;
use crate::location::LocationId;
use crate::policy::{BasicPolicy, CompanyPolicy};
use crate::rocket::RocketId;
use crate::ui::modals::help_tests::render;
use crate::ui::{
    App, DesignerSubMode, DvPlannerState, EditorField, InputMode, LocationPickerTarget, PickSlot,
    PlannerSetupField, PlannerSetupState, PlannerSource, RocketDesignerState, Tab,
};

/// One company with a bit of everything: the bot runs it for two and a
/// half years (engine and rocket projects in every state, a
/// manufacturing queue, contracts, launches, award history), then it
/// gets a reactor project, two parked spacecraft at the same orbit (so
/// docking has a candidate), a rocket in flight to the Moon, a campaign
/// soliciting bids, and one rocket in inventory to launch.
fn rich_app() -> App {
    let mut game = crate::game_state::GameState::new("Smoke Co".into(), 250_000_000.0, 11);
    let mut policy = BasicPolicy::new();
    for _ in 0..900 {
        policy.act(&mut game);
        game.advance_day();
    }
    let bal = game.balance.clone();
    let company = &mut game.player_company;
    assert!(!company.engine_projects.is_empty() && !company.rocket_projects.is_empty(),
        "fixture premise: the bot has projects after 900 days");
    company.start_proposed_reactor("Smoke Reactor".into(), 1.0, crate::reactor::EnrichmentLevel::Leu, &bal);

    let rp = company.rocket_projects[0].clone();
    // Two spacecraft parked in LEO, one carrying a payload.
    for (i, name) in ["Parked One", "Parked Two"].iter().enumerate() {
        let rocket = rp.design.instantiate(RocketId(500 + i as u64), 0.0);
        game.spacecraft.push(crate::game_state::Spacecraft {
            id: crate::game_state::SpacecraftId(500 + i as u64),
            name: (*name).into(),
            rocket,
            design: rp.design.clone(),
            location: LocationId::of("leo"),
            rocket_project_id: rp.project_id,
            payloads: if i == 0 { vec![Payload::TestMass { mass_kg: 50.0 }] } else { Vec::new() },
        });
    }
    // A rocket in inventory to launch, and one already on its way to the Moon.
    let inventory_id = |n: u64| crate::manufacturing::InventoryItemId(9_000 + n);
    for n in 0..2 {
        game.player_company.manufacturing.inventory.rockets.push(crate::manufacturing::InventoryRocket {
            item_id: inventory_id(n),
            rocket_project_id: rp.project_id,
            design_id: rp.design.id,
            rocket_name: rp.design.name.clone(),
            build_cost: 15_000_000.0,
            revision: rp.revision,
            rocket_flaws: Vec::new(),
        });
    }
    let launched = game.launch_rocket(inventory_id(1), "lunar_orbit", vec![Payload::TestMass { mass_kg: 100.0 }], true);
    assert!(launched.is_some(), "fixture premise: the inventory rocket launches");
    // A campaign soliciting bids.
    let market = &game.markets[0];
    let today = game.date;
    game.active_campaigns.push(Campaign {
        id: CampaignId(7_001),
        name: "Smoke Constellation".into(),
        market_id: market.id,
        destination: "leo".into(),
        destination_display: "Low Earth Orbit".into(),
        payload_kg: 600.0,
        payment_per_mission: 20_000_000.0,
        missions_total: 4,
        missions_issued: 0,
        missions_missed: 0,
        next_issue_date: today.add_months(2),
        interval_days: 60,
        status: CampaignStatus::Soliciting {
            bid_deadline: today.add_months(1),
            budget_ceiling_per_mission: 25_000_000.0,
            player_bid: None,
        },
    });
    App::new(game)
}

/// A designer state holding the bot's two-stage design.
fn designer_state(app: &App) -> Box<RocketDesignerState> {
    let rp = &app.game.player_company.rocket_projects[0];
    let pid = app.game.player_company.engine_projects[0].project_id;
    let mut state = Box::new(RocketDesignerState::new("Smoke Design".into()));
    for group in &rp.design.stage_groups {
        state.push_new_group(group[0].clone(), crate::ui::EngineSource::PlayerDesign(pid));
    }
    state
}

/// Every modal the fixture can reach, named.
fn modes(app: &App) -> Vec<(&'static str, InputMode)> {
    let company = &app.game.player_company;
    let epid = company.engine_projects[0].project_id;
    let rpid = company.reactor_projects[0].project_id;
    let rocket_ref = crate::company::ProjectRef::Rocket(company.rocket_projects[0].project_id);
    let effects = company.retirement_plan(rocket_ref).expect("a rocket can be retired");
    let locations: Vec<(&'static str, &'static str)> = crate::location::DELTA_V_MAP.locations()
        .iter().map(|l| (l.id, l.display_name)).collect();
    let record = company.launch_history.first().cloned().expect("fixture premise: a launch happened");
    let sc_design = app.game.spacecraft[0].design.clone();
    let sc_rocket = app.game.spacecraft[0].rocket.clone();
    let inventory_item = company.manufacturing.inventory.rockets[0].item_id;
    let campaign_id = app.game.active_campaigns[0].id;
    let state = || designer_state(app);
    vec![
        ("help", InputMode::Help { tab: 0 }),
        ("designer help", InputMode::designer_with(state(), DesignerSubMode::Help)),
        ("intro", InputMode::Intro),
        ("engine editor", InputMode::EngineEditor { project_id: epid, cursor: 1, state: None }),
        ("engine editor from designer", InputMode::EngineEditor { project_id: epid, cursor: 1, state: Some(state()) }),
        ("engine name", InputMode::EngineEditorField {
            project_id: epid, cursor: 0, field: EditorField::Name, buffer: "Nam".into(), state: None,
        }),
        ("engine scale", InputMode::EngineEditorField {
            project_id: epid, cursor: 3, field: EditorField::Scale, buffer: "1.5".into(), state: None,
        }),
        ("reactor editor", InputMode::ReactorEditor { project_id: rpid, cursor: 0 }),
        ("reactor name", InputMode::ReactorEditorField {
            project_id: rpid, cursor: 0, field: EditorField::Name, buffer: "R".into(),
        }),
        ("reactor scale", InputMode::ReactorEditorField {
            project_id: rpid, cursor: 1, field: EditorField::Scale, buffer: "2".into(),
        }),
        ("third party", InputMode::SelectThirdParty { selected: 0 }),
        ("rocket name", InputMode::RocketName { buffer: "Smoke".into() }),
        ("bid entry", InputMode::BidEntry { contract_index: 0, buffer: "12".into() }),
        ("bid rules", InputMode::BidRules { selected: 0 }),
        ("award history", InputMode::AwardHistory { scroll: 0 }),
        ("campaigns", InputMode::Campaigns { selected: 0 }),
        ("campaign bid", InputMode::CampaignBidEntry { campaign_id, selected: 0, buffer: "18".into() }),
        ("designer", InputMode::designer(state())),
        ("power editor", InputMode::designer_with(
            state(), DesignerSubMode::PowerEditor { group_index: 0, stage_index: 0, cursor: 0 },
        )),
        ("engine picker", InputMode::designer_with(state(), DesignerSubMode::pick(PickSlot::Append))),
        ("payload input", InputMode::designer_with(state(), DesignerSubMode::PayloadInput { buffer: "500".into() })),
        ("location picker", InputMode::designer_with(state(), DesignerSubMode::LocationPicker {
            target: LocationPickerTarget::LaunchSite, locations: locations.clone(), selected: 0,
        })),
        ("destination picker", InputMode::designer_with(state(), DesignerSubMode::LocationPicker {
            target: LocationPickerTarget::MissionDestination, locations: locations.clone(), selected: 3,
        })),
        ("launch manifest", InputMode::LaunchManifest {
            rocket_item_id: inventory_item, persist: false,
            contract_picks: vec![false; app.game.available_contracts.len()],
            spacecraft_picks: Vec::new(), spacecraft_item_ids: Vec::new(), cursor: 0,
        }),
        ("launch result", InputMode::LaunchResult { record }),
        ("fly select spacecraft", InputMode::FlySelectSpacecraft { selected: 0 }),
        ("dock small", InputMode::DockSelectSmall { selected: 0 }),
        ("dock large", InputMode::DockSelectLarge { small_idx: 0, candidates: vec![1], selected: 0 }),
        ("undock carrier", InputMode::UndockSelectCarrier { candidates: vec![0], selected: 0 }),
        ("undock payload", InputMode::UndockSelectPayload { carrier_idx: 0, payload_indices: vec![0], selected: 0 }),
        ("fly destination", InputMode::FlySelectDestination {
            spacecraft_index: 0,
            destinations: vec![("gto".into(), "Geostationary Transfer".into(), 2_440.0)],
            remaining_dv: 5_000.0, selected: 0,
        }),
        ("planner setup", InputMode::PlannerSetup { state: Box::new(PlannerSetupState {
            eligible_projects: vec![0], selected_project: 0, locations: locations.clone(),
            selected_location: 0, payload_buffer: "1000".into(), active_field: PlannerSetupField::Location,
        }) }),
        ("dv planner", InputMode::DvPlanner { state: Box::new(DvPlannerState {
            source: PlannerSource::Spacecraft { spacecraft_index: 0 },
            rocket: sc_rocket, design: sc_design,
            current_location: LocationId::of("leo"), actions: Vec::new(), snapshots: Vec::new(),
            destinations: vec![("gto".into(), "Geostationary Transfer".into(), 2_440.0)],
            selected: 0, payload_kg: 50.0,
        }) }),
        ("confirm retire", InputMode::ConfirmRetire { target: rocket_ref, effects }),
    ]
}

/// Every tab, every modal, both widths: nothing panics and every
/// screen draws something.
#[test]
fn every_tab_and_modal_renders_at_both_widths() {
    let mut app = rich_app();
    let sizes = [(80u16, 30u16), (120, 40)];
    for (ti, tab) in Tab::ALL.iter().enumerate() {
        app.active_tab = ti;
        app.selected_item = 0;
        app.input_mode = InputMode::Normal;
        for &(w, h) in &sizes {
            let screen = render(&app, w, h);
            assert!(screen.contains(tab.name()) || !screen.trim().is_empty(),
                "{tab:?} at {w} cols drew nothing");
        }
    }
    app.active_tab = Tab::ALL.iter().position(|t| matches!(t, Tab::Contracts)).unwrap_or(0);
    let modes = modes(&app);
    for (name, mode) in modes {
        app.input_mode = mode;
        for &(w, h) in &sizes {
            let screen = render(&app, w, h);
            assert!(!screen.trim().is_empty(), "modal {name:?} at {w} cols drew nothing");
        }
    }
    // The launches tab, in flight panel, and the spacecraft roster
    // have the most arithmetic behind them; scroll through them too.
    app.input_mode = InputMode::Normal;
    app.active_tab = Tab::ALL.iter().position(|t| matches!(t, Tab::Launches)).unwrap_or(0);
    for item in 0..4 {
        app.selected_item = item;
        let _ = render(&app, 120, 40);
    }
    // And a few keys through the designer, which is where the last
    // regression was found.
    app.input_mode = InputMode::designer(designer_state(&app));
    for key in [KeyCode::Down, KeyCode::Char('v'), KeyCode::Char('b'), KeyCode::Esc, KeyCode::Char('?')] {
        app.handle_key(key);
        let _ = render(&app, 120, 40);
    }
    assert!(matches!(app.input_mode, InputMode::RocketDesigner { sub: DesignerSubMode::Help, .. }));
    app.handle_key(KeyCode::Char('z'));
    assert!(matches!(app.input_mode, InputMode::RocketDesigner { sub: DesignerSubMode::Main, .. }),
        "any key closes the designer's help back into the designer");
}

/// The designer's sub-modals draw over the designer, not over the tabs
/// behind it (17_4_UI.md Q1), and so does the engine editor opened from
/// the designer's picker: the designer's title is still on screen under
/// each of them.
#[test]
fn designer_sub_modals_draw_over_the_designer() {
    let app0 = rich_app();
    let mut app = rich_app();
    let epid = app.game.player_company.engine_projects[0].project_id;
    let locations: Vec<(&'static str, &'static str)> = crate::location::DELTA_V_MAP.locations()
        .iter().map(|l| (l.id, l.display_name)).collect();
    let title = "Rocket Designer: \"Smoke Design\"";
    let subs = vec![
        ("engine picker", DesignerSubMode::pick(PickSlot::Append)),
        ("payload input", DesignerSubMode::PayloadInput { buffer: "500".into() }),
        ("location picker", DesignerSubMode::LocationPicker {
            target: LocationPickerTarget::LaunchSite, locations, selected: 0,
        }),
        ("power editor", DesignerSubMode::PowerEditor { group_index: 0, stage_index: 0, cursor: 0 }),
        ("help", DesignerSubMode::Help),
    ];
    for (name, sub) in subs {
        app.input_mode = InputMode::designer_with(designer_state(&app0), sub);
        let screen = render(&app, 120, 40);
        assert!(screen.contains(title), "{name}: the designer should be drawn behind it");
    }
    for (name, mode) in [
        ("engine editor", InputMode::EngineEditor { project_id: epid, cursor: 0, state: Some(designer_state(&app0)) }),
        ("engine name", InputMode::EngineEditorField {
            project_id: epid, cursor: 0, field: EditorField::Name, buffer: "N".into(), state: Some(designer_state(&app0)),
        }),
    ] {
        app.input_mode = mode;
        let screen = render(&app, 120, 40);
        assert!(screen.contains(title), "{name} from the designer: the designer should be drawn behind it");
    }
    // And a standalone engine editor does not drag the designer in.
    app.input_mode = InputMode::EngineEditor { project_id: epid, cursor: 0, state: None };
    assert!(!render(&app, 120, 40).contains(title));
}
