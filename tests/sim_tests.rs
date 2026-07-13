//! Pure-simulation tests: determinism, invariants, command validation and the
//! wire protocol framing.

use std::io::Cursor;

use frozen_city::game::sim;
use frozen_city::game::types::*;
use frozen_city::net::protocol::{read_frame, write_frame, ClientMsg, Included, ServerMsg};

#[test]
fn mapgen_is_deterministic() {
    let a = sim::new_game(1234, 12);
    let b = sim::new_game(1234, 12);
    assert_eq!(a, b);
    let c = sim::new_game(9999, 12);
    assert_ne!(a.tiles, c.tiles, "different seeds must differ");
}

#[test]
fn tile_lookup_survives_delta_snapshot_state() {
    // A raw protocol consumer holds `tiles: Vec::new()` between full
    // snapshots (`Included { tiles: false }` ships the grid empty) — tile()
    // must answer None there, not panic on the indexing.
    let mut state = sim::new_game(7, 12);
    state.tiles = Vec::new();
    assert!(state.tile(0, 0).is_none());
    assert!(state.tile(30, 30).is_none());
}

#[test]
fn mapgen_has_resources_and_clear_center() {
    let state = sim::new_game(7, 12);
    let forest = state
        .tiles
        .iter()
        .filter(|t| t.terrain == Terrain::Forest)
        .count();
    let coal = state
        .tiles
        .iter()
        .filter(|t| t.terrain == Terrain::Coal)
        .count();
    assert!(forest > 50, "expected a real forest, got {forest}");
    assert!(coal > 10, "expected coal deposits, got {coal}");
    // The furnace area itself must stay clear.
    for y in 29..=34u8 {
        for x in 29..=34u8 {
            if GameState::dist_to_furnace(x, y) < 4.0 {
                assert_eq!(state.tile(x, y).expect("in bounds").terrain, Terrain::Snow);
            }
        }
    }
    assert_eq!(state.buildings.len(), 1);
    assert_eq!(state.buildings[0].kind, BuildingKind::Furnace);
    assert_eq!(state.survivors.len(), 8);
}

#[test]
fn simulation_is_deterministic() {
    let mut a = sim::new_game(42, 12);
    let mut b = sim::new_game(42, 12);
    for _ in 0..2000 {
        sim::tick(&mut a);
        sim::tick(&mut b);
    }
    assert_eq!(a, b);
}

#[test]
fn long_run_invariants_hold() {
    let mut state = sim::new_game(2024, 30);
    for _ in 0..10_000 {
        sim::tick(&mut state);
        assert!(state.stock.wood >= -0.001, "wood went negative");
        assert!(state.stock.coal >= -0.001, "coal went negative");
        assert!(state.stock.food >= -0.001, "food went negative");
        assert!(state.survivors.len() <= 60);
        assert!(
            state.total_workers() <= state.survivors.len() as u32,
            "more workers than people"
        );
        for s in &state.survivors {
            assert!(s.hp > 0.0 && s.hp <= 100.0);
        }
        if state.phase != GamePhase::Running {
            break;
        }
    }
}

#[test]
fn placing_a_tent_costs_wood_and_blocks_the_spot() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let wood_before = state.stock.wood;

    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 2);
    assert!((state.stock.wood - (wood_before - 15.0)).abs() < 0.001);

    // Same spot again: occupied, silently rejected, no wood spent.
    let wood_after = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 2);
    assert_eq!(state.stock.wood, wood_after);
}

#[test]
fn cannot_build_without_wood_or_on_bad_terrain() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    state.stock.wood = 3.0;
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 1, "no wood -> no tent");

    state.stock.wood = 100.0;
    // A coal mine demands a coal deposit tile.
    assert!(state.can_place(BuildingKind::CoalMine, x, y).is_err());
    // But works on an actual deposit.
    let (cx, cy) = find_spot(&state, BuildingKind::CoalMine);
    assert!(state.can_place(BuildingKind::CoalMine, cx, cy).is_ok());
}

#[test]
fn worker_assignment_is_clamped() {
    let mut state = sim::new_game(5, 12);
    state.stock.wood = 200.0;
    let (x, y) = find_spot(&state, BuildingKind::Sawmill);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let id = state.buildings.last().unwrap().id;

    // V0.8: qurilish paytida sig'im — brigada capi.
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 100 });
    assert_eq!(
        state.find_building(id).unwrap().workers,
        CONSTRUCTION_CREW_MAX,
        "a construction site clamps to the crew cap"
    );

    // Bitgach — binoning o'z maksimumi.
    sim::finish_all_construction(&mut state);
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 100 });
    assert_eq!(
        state.find_building(id).unwrap().workers,
        BuildingKind::Sawmill.max_workers(),
        "clamped to the building maximum"
    );
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: -100 });
    assert_eq!(state.find_building(id).unwrap().workers, 0);
}

#[test]
fn sawmill_produces_wood_and_eats_the_forest() {
    let mut state = sim::new_game(5, 12);
    state.stock.wood = 100.0;
    let (x, y) = find_sawmill_spot_near_forest(&state);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Sawmill, x, y });
    let id = state.buildings.last().unwrap().id;
    sim::apply_command(&mut state, 1, &PlayerCommand::AdjustWorkers { building: id, delta: 2 });

    let wood_before = state.stock.wood;
    let forest_before = state.forest_near(x, y, 4);
    for _ in 0..(TICKS_PER_DAY as usize) {
        sim::tick(&mut state);
    }
    // Furnace may burn wood if coal runs dry, so compare against production.
    let forest_after = state.forest_near(x, y, 4);
    assert!(
        forest_after < forest_before,
        "forest should be consumed ({forest_before} -> {forest_after})"
    );
    assert!(
        state.stock.wood > wood_before - 40.0,
        "wood income should offset most consumption"
    );
}

#[test]
fn demolish_refunds_and_furnace_is_protected() {
    let mut state = sim::new_game(5, 12);
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    let id = state.buildings.last().unwrap().id;
    let wood = state.stock.wood;
    sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: id });
    assert_eq!(state.buildings.len(), 1);
    assert!((state.stock.wood - (wood + 6.0)).abs() < 0.001, "40% of 15");

    sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: 0 });
    assert_eq!(state.buildings.len(), 1, "the furnace cannot be demolished");
}

#[test]
fn furnace_goes_out_without_fuel() {
    let mut state = sim::new_game(5, 12);
    state.stock.coal = 0.0;
    state.stock.wood = 0.0;
    assert!(state.furnace_lit);
    sim::tick(&mut state);
    assert!(!state.furnace_lit, "no fuel, no fire");
    assert!(state.heat_radius() == 0.0);
}

#[test]
fn city_survives_to_victory_on_short_run() {
    let mut state = sim::new_game(31337, 1);
    for _ in 0..(TICKS_PER_DAY * 2) {
        sim::tick(&mut state);
        if state.phase == GamePhase::Won {
            break;
        }
    }
    assert_eq!(state.phase, GamePhase::Won);
}

#[test]
fn freezing_city_is_lost() {
    let mut state = sim::new_game(11, 12);
    state.furnace_level = 0;
    state.stock = Stockpile { wood: 0.0, coal: 0.0, food: 0.0 };
    for s in &mut state.survivors {
        s.hp = 0.5;
        s.hunger = 100.0;
    }
    for _ in 0..(TICKS_PER_DAY * 4) {
        sim::tick(&mut state);
        if state.phase == GamePhase::Lost {
            break;
        }
    }
    assert_eq!(state.phase, GamePhase::Lost);
    assert!(state.survivors.is_empty());
}

#[test]
fn commands_after_game_end_are_ignored() {
    let mut state = sim::new_game(5, 12);
    state.phase = GamePhase::Lost;
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    assert_eq!(state.buildings.len(), 1);
}

#[test]
fn protocol_frames_roundtrip() {
    let state = sim::new_game(77, 12);
    let msgs = vec![
        ServerMsg::Welcome { player_id: 3, token: 3, state: state.clone() },
        ServerMsg::State {
            state,
            included: Included {
                tiles: true,
                events: true,
                chat: true,
                pings: true,
                missions: true,
                techs: true,
            },
        },
    ];
    let mut buf = Vec::new();
    for m in &msgs {
        write_frame(&mut buf, m).unwrap();
    }
    let mut cur = Cursor::new(buf);
    for m in &msgs {
        let back: ServerMsg = read_frame(&mut cur).unwrap();
        match (m, &back) {
            (
                ServerMsg::Welcome { player_id: a, token: ta, state: sa },
                ServerMsg::Welcome { player_id: b, token: tb, state: sb },
            ) => {
                assert_eq!(a, b);
                assert_eq!(ta, tb);
                assert_eq!(sa, sb);
            }
            (ServerMsg::State { state: sa, .. }, ServerMsg::State { state: sb, .. }) => {
                assert_eq!(sa, sb);
            }
            _ => panic!("message kind mismatch"),
        }
    }

    let cmd = ClientMsg::Cmd(PlayerCommand::Place { kind: BuildingKind::Sawmill, x: 10, y: 20 });
    let mut buf = Vec::new();
    write_frame(&mut buf, &cmd).unwrap();
    let back: ClientMsg = read_frame(&mut Cursor::new(buf)).unwrap();
    match back {
        ClientMsg::Cmd(PlayerCommand::Place { kind, x, y }) => {
            assert_eq!(kind, BuildingKind::Sawmill);
            assert_eq!((x, y), (10, 20));
        }
        _ => panic!("wrong message"),
    }
}

#[test]
fn frame_reader_never_panics_on_garbage() {
    use frozen_city::game::rng::Rng;

    let mut rng = Rng::new(0xC0FFEE);
    for _ in 0..5000 {
        let len = rng.below(513) as usize; // 0..=512 bytes, keeps this fast
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            buf.push(rng.below(256) as u8);
        }
        // Never panics: malformed/short/garbage input must come back as Err.
        let _ = read_frame::<_, ClientMsg>(&mut Cursor::new(buf.clone()));
        let _ = read_frame::<_, ServerMsg>(&mut Cursor::new(buf));
    }
}

#[test]
fn attribution_on_place_credits_the_player() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    let (x, y) = find_spot(&state, BuildingKind::Tent);

    sim::apply_command(&mut state, 7, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });

    let built = state.buildings.last().unwrap();
    assert_eq!(built.owner, Some(7));
    assert_eq!(state.player(7).unwrap().built, 1);
    assert!(
        state.events.last().unwrap().text.contains("Zara"),
        "event should attribute the build to Zara: {:?}",
        state.events.last()
    );
}

#[test]
fn attribution_on_demolish_credits_the_player() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    let (x, y) = find_spot(&state, BuildingKind::Tent);
    sim::apply_command(&mut state, 7, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
    let id = state.buildings.last().unwrap().id;

    sim::apply_command(&mut state, 7, &PlayerCommand::Demolish { building: id });

    assert_eq!(state.player(7).unwrap().demolished, 1);
    assert!(
        state.events.last().unwrap().text.contains("Zara"),
        "event should attribute the demolition to Zara: {:?}",
        state.events.last()
    );
}

#[test]
fn push_chat_appends_a_line_for_a_known_player() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    sim::push_chat(&mut state, 7, "hello team");

    assert_eq!(state.chat.len(), 1);
    assert_eq!(state.chat[0].player_id, 7);
    assert_eq!(state.chat[0].name, "Zara");
    assert_eq!(state.chat[0].text, "hello team");
    assert_eq!(state.total_chat, 1);
}

#[test]
fn push_chat_ignores_an_unregistered_player() {
    let mut state = sim::new_game(5, 12);
    sim::push_chat(&mut state, 999, "ghost message");
    assert!(state.chat.is_empty());
    assert_eq!(state.total_chat, 0);
}

#[test]
fn push_chat_drops_whitespace_only_text() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    sim::push_chat(&mut state, 7, "   \t   ");
    assert!(state.chat.is_empty());
    assert_eq!(state.total_chat, 0);
}

#[test]
fn push_chat_caps_overlong_text() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    let long_text = "x".repeat(MAX_CHAT_LEN * 2);

    sim::push_chat(&mut state, 7, &long_text);

    assert_eq!(state.chat.len(), 1);
    assert!(state.chat[0].text.chars().count() <= MAX_CHAT_LEN);
}

#[test]
fn push_chat_rolls_the_log_but_keeps_counting_total() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    let sent = MAX_CHAT + 10;
    for i in 0..sent {
        sim::push_chat(&mut state, 7, &format!("line {i}"));
    }

    assert_eq!(state.chat.len(), MAX_CHAT, "log stays capped");
    assert_eq!(state.total_chat, sent as u64, "counter keeps growing");
}

#[test]
fn add_ping_is_recorded_and_expires_after_ttl() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    sim::add_ping(&mut state, 7, 10.0, 20.0);
    assert_eq!(state.pings.len(), 1);
    assert_eq!(state.pings[0].player_id, 7);

    for _ in 0..(PING_TTL_TICKS as usize + 1) {
        sim::tick(&mut state);
    }
    assert!(state.pings.is_empty(), "ping should have expired");
}

#[test]
fn add_ping_caps_per_player_and_globally() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    // A single spammer is bounded to their own per-player cap, so they cannot
    // fill the whole shared ping list and evict everyone else's markers.
    for i in 0..(MAX_PINGS + 5) {
        sim::add_ping(&mut state, 7, i as f32, i as f32);
    }
    assert_eq!(
        state.pings.iter().filter(|p| p.player_id == 7).count(),
        MAX_PINGS_PER_PLAYER
    );
    assert!(state.pings.len() <= MAX_PINGS);

    // Many players together are still bounded by the global cap.
    for pid in 10..20u64 {
        sim::player_joined(&mut state, pid, "P");
        for _ in 0..MAX_PINGS_PER_PLAYER {
            sim::add_ping(&mut state, pid, pid as f32, pid as f32);
        }
    }
    assert_eq!(state.pings.len(), MAX_PINGS);
}

#[test]
fn add_ping_is_ignored_after_game_over() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    state.phase = GamePhase::Lost;
    sim::add_ping(&mut state, 7, 5.0, 5.0);
    assert!(state.pings.is_empty(), "no pings on a frozen, game-over world");
}

#[test]
fn push_chat_strips_bidi_override() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");
    // U+202E RIGHT-TO-LEFT OVERRIDE and a zero-width joiner must be stripped.
    sim::push_chat(&mut state, 7, "hi\u{202E}evil\u{200D}there");
    assert_eq!(state.chat.len(), 1);
    let line = &state.chat[0];
    assert!(!line.text.contains('\u{202E}'), "bidi override survived");
    assert!(!line.text.contains('\u{200D}'), "zero-width joiner survived");
    assert_eq!(line.text, "hievilthere");
}

#[test]
fn player_color_reuses_a_freed_slot() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "A"); // color 0
    sim::player_joined(&mut state, 2, "B"); // color 1
    sim::player_joined(&mut state, 3, "C"); // color 2
    sim::player_left(&mut state, 2); // frees color 1
    sim::player_joined(&mut state, 4, "D"); // should take freed color 1
    let c = state.player(3).unwrap().color;
    let d = state.player(4).unwrap().color;
    assert_ne!(c, d, "a new player must not reuse a still-connected player's color");
    assert_eq!(d, 1, "lowest free slot");
}

#[test]
fn player_rejoined_restores_identity_and_stats() {
    let mut state = sim::new_game(5, 12);
    let saved = PlayerInfo {
        id: 5,
        name: "Ivo".into(),
        color: 2,
        cursor: Some((3.0, 4.0)),
        built: 3,
        demolished: 1,
        role: Role::Guest,
        account: None,
    };

    sim::player_rejoined(&mut state, saved);

    let p = state.player(5).expect("player restored to the roster");
    assert_eq!(p.name, "Ivo");
    assert_eq!(p.color, 2);
    assert_eq!(p.built, 3);
    assert_eq!(p.demolished, 1);
    assert!(p.cursor.is_none(), "cursor resets on rejoin");
    assert!(
        state.events.last().unwrap().text.contains("reconnected"),
        "expected a reconnect event: {:?}",
        state.events.last()
    );
}

/// The co-op additions (chat/pings/attribution) must not touch the sim RNG:
/// two fresh worlds ticked identically with no chat/ping traffic must stay
/// byte-identical, exactly as before these features existed.
#[test]
fn coop_additions_do_not_break_determinism() {
    let mut a = sim::new_game(4242, 12);
    let mut b = sim::new_game(4242, 12);
    for _ in 0..2000 {
        sim::tick(&mut a);
        sim::tick(&mut b);
    }
    assert_eq!(a, b);
}

// --- helpers ---

fn find_spot(state: &GameState, kind: BuildingKind) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_place(kind, x, y).is_ok() {
                return (x, y);
            }
        }
    }
    panic!("no valid spot for {kind:?}");
}

fn find_sawmill_spot_near_forest(state: &GameState) -> (u8, u8) {
    for y in 0..MAP_H as u8 {
        for x in 0..MAP_W as u8 {
            if state.can_place(BuildingKind::Sawmill, x, y).is_ok()
                && state.forest_near(x, y, 4) > 100
            {
                return (x, y);
            }
        }
    }
    panic!("no sawmill spot near forest");
}
