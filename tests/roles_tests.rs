//! Pure-simulation tests for the roles/ownership system: who owns a world,
//! what guests may do under each `GuestPermission`, and the two related
//! robustness fixes (system-event eviction protection, zalgo-mark capping in
//! chat). Mirrors the style of `sim_tests.rs`.

use frozen_city::game::sim;
use frozen_city::game::types::*;

#[test]
fn player_joined_assigns_owner_to_first_and_guest_to_rest() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Anna");
    sim::player_joined(&mut state, 2, "Boris");
    sim::player_joined(&mut state, 3, "Clara");

    assert_eq!(state.player(1).unwrap().role, Role::Owner, "first joiner owns the world");
    assert_eq!(state.player(2).unwrap().role, Role::Guest);
    assert_eq!(state.player(3).unwrap().role, Role::Guest);
}

#[test]
fn can_issue_allows_unknown_player_ids() {
    let state = sim::new_game(5, 12);
    // No player registered in the roster at all — unit tests and trusted
    // local/tool calls rely on unknown pids being unrestricted.
    assert!(state.can_issue(1, &PlayerCommand::SetFurnaceLevel { level: 2 }));
    assert!(state.can_issue(999, &PlayerCommand::Demolish { building: 0 }));
}

#[test]
fn can_issue_owner_may_do_everything() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert!(state.is_owner(1));

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    assert!(state.can_issue(1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y }));
    assert!(state.can_issue(1, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }));
    assert!(state.can_issue(1, &PlayerCommand::Demolish { building: 0 }));
    assert!(state.can_issue(1, &PlayerCommand::SetFurnaceLevel { level: 2 }));
}

#[test]
fn can_issue_guest_view_only_denies_everything() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    assert!(!state.can_issue(2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y }));
    assert!(!state.can_issue(2, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }));
    assert!(!state.can_issue(2, &PlayerCommand::Demolish { building: 0 }));
    assert!(!state.can_issue(2, &PlayerCommand::SetFurnaceLevel { level: 2 }));
}

#[test]
fn can_issue_guest_build_may_place_and_adjust_workers() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert_eq!(state.guest_perm, GuestPermission::Build, "default policy is Build");

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    assert!(state.can_issue(2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y }));
    assert!(state.can_issue(2, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }));
}

#[test]
fn can_issue_guest_build_may_demolish_own_building_but_not_others() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");

    let (gx, gy) = find_spot(&state, BuildingKind::Tent);
    state.buildings.push(Building {
        id: 101,
        kind: BuildingKind::Tent,
        x: gx,
        y: gy,
        workers: 0,
        progress: 0.0,
        owner: Some(2),
        owner_account: None,
    });
    let (ox, oy) = find_spot(&state, BuildingKind::Tent);
    state.buildings.push(Building {
        id: 102,
        kind: BuildingKind::Tent,
        x: ox,
        y: oy,
        workers: 0,
        progress: 0.0,
        owner: Some(1),
        owner_account: None,
    });

    assert!(state.can_issue(2, &PlayerCommand::Demolish { building: 101 }), "guest may demolish their own building");
    assert!(!state.can_issue(2, &PlayerCommand::Demolish { building: 102 }), "guest may not demolish someone else's building");
}

#[test]
fn can_issue_guest_build_may_assign_named_survivors() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert_eq!(state.guest_perm, GuestPermission::Build, "default policy is Build");

    assert!(state.can_issue(
        2,
        &PlayerCommand::AssignSurvivor { survivor: 1, building: Some(0) }
    ));
}

#[test]
fn can_issue_guest_view_only_denies_assign_survivor() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);

    assert!(!state.can_issue(
        2,
        &PlayerCommand::AssignSurvivor { survivor: 1, building: Some(0) }
    ));
}

#[test]
fn can_issue_owner_may_assign_survivors() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");

    assert!(state.can_issue(
        1,
        &PlayerCommand::AssignSurvivor { survivor: 1, building: Some(0) }
    ));
}

#[test]
fn can_issue_guest_build_may_not_set_furnace_level() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert!(!state.can_issue(2, &PlayerCommand::SetFurnaceLevel { level: 2 }));
}

#[test]
fn can_issue_guest_full_may_do_everything() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    sim::set_guest_permission(&mut state, GuestPermission::Full);

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    assert!(state.can_issue(2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y }));
    assert!(state.can_issue(2, &PlayerCommand::AdjustWorkers { building: 0, delta: 1 }));
    assert!(state.can_issue(2, &PlayerCommand::Demolish { building: 0 }));
    assert!(state.can_issue(2, &PlayerCommand::SetFurnaceLevel { level: 2 }));
}

#[test]
fn can_issue_owner_less_world_allows_guest_everything() {
    let mut state = sim::new_game(5, 12);
    // `player_joined` always makes the first joiner Owner, so to exercise the
    // "owner disconnected, only guests remain" scenario, join then flip the
    // role directly (GameState's fields are public) — exactly the situation
    // `owner_present` exists to detect.
    sim::player_joined(&mut state, 2, "Guest");
    state.players[0].role = Role::Guest;
    assert!(!state.owner_present());

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    assert!(state.can_issue(2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y }));
    assert!(state.can_issue(2, &PlayerCommand::SetFurnaceLevel { level: 2 }));
}

#[test]
fn apply_command_enforces_view_only_guest_place_is_a_noop() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);

    let (x, y) = find_spot(&state, BuildingKind::Tent);
    let wood_before = state.stock.wood;
    let buildings_before = state.buildings.len();

    sim::apply_command(&mut state, 2, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });

    assert_eq!(state.buildings.len(), buildings_before, "guest Place must be rejected under ViewOnly");
    assert_eq!(state.stock.wood, wood_before, "no wood spent on a rejected command");
}

#[test]
fn action_event_spam_does_not_evict_the_system_event() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    state.stock.wood = 10_000.0;

    assert!(
        state.events.iter().any(|e| e.system),
        "new_game pushes a system welcome event"
    );

    // Place-then-demolish as the owner, repeatedly: each cycle pushes two
    // cosmetic (non-system) action events, well past the 12-event cap.
    for _ in 0..8 {
        let (x, y) = find_spot(&state, BuildingKind::Tent);
        sim::apply_command(&mut state, 1, &PlayerCommand::Place { kind: BuildingKind::Tent, x, y });
        let id = state.buildings.last().unwrap().id;
        sim::apply_command(&mut state, 1, &PlayerCommand::Demolish { building: id });
    }

    assert!(state.total_events > 12, "enough events were pushed to overflow the cap");
    assert!(state.events.len() <= 12, "log stays capped");
    assert!(
        state.events.iter().any(|e| e.system),
        "a system event must survive cosmetic action-event spam: {:?}",
        state.events
    );
}

#[test]
fn push_chat_caps_stacked_combining_marks() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    let mut text = String::from("e");
    for _ in 0..20 {
        text.push('\u{0301}'); // combining acute accent (zalgo-style stacking)
    }

    sim::push_chat(&mut state, 7, &text);

    assert_eq!(state.chat.len(), 1);
    let stored = &state.chat[0].text;
    assert!(stored.starts_with('e'), "base character preserved: {stored:?}");
    let combining_count = stored.chars().filter(|c| is_combining_mark(*c)).count();
    assert!(
        combining_count <= 2,
        "expected at most ~2 stacked combining marks, got {combining_count}: {stored:?}"
    );
}

#[test]
fn set_guest_permission_updates_state() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    assert_eq!(state.guest_perm, GuestPermission::Build);

    sim::set_guest_permission(&mut state, GuestPermission::ViewOnly);
    assert_eq!(state.guest_perm, GuestPermission::ViewOnly);

    sim::set_guest_permission(&mut state, GuestPermission::Full);
    assert_eq!(state.guest_perm, GuestPermission::Full);
}

#[test]
fn kick_player_removes_from_roster_and_pushes_event() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    sim::player_joined(&mut state, 2, "Guest");
    assert_eq!(state.players.len(), 2);

    sim::kick_player(&mut state, 2);

    assert_eq!(state.players.len(), 1);
    assert!(state.player(2).is_none());
    assert!(
        state.events.last().unwrap().text.contains("removed"),
        "expected a kick event: {:?}",
        state.events.last()
    );
}

#[test]
fn kick_player_is_a_noop_for_a_missing_target() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    let events_before = state.events.len();

    sim::kick_player(&mut state, 999);

    assert_eq!(state.players.len(), 1);
    assert_eq!(state.events.len(), events_before, "no event for a no-op kick");
}

#[test]
fn a_parked_owner_keeps_ownership_from_a_fresh_joiner() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 1, "Owner");
    assert_eq!(state.player(1).unwrap().role, Role::Owner);

    // The owner's connection drops (roster empties) but their session is parked
    // for reconnect — ownership must not evaporate with the live roster.
    sim::player_left(&mut state, 1);
    assert!(state.players.is_empty());
    assert_eq!(state.owner_id, Some(1), "ownership is remembered while parked");

    // A stranger joins the momentarily-empty world — must NOT seize ownership.
    sim::player_joined(&mut state, 2, "Stranger");
    assert_eq!(
        state.player(2).unwrap().role,
        Role::Guest,
        "an empty roster must not grant ownership to a fresh joiner"
    );

    // The real owner returns and regains Owner.
    sim::player_joined(&mut state, 1, "Owner");
    assert_eq!(state.player(1).unwrap().role, Role::Owner);
    assert_eq!(
        state.players.iter().filter(|p| p.role == Role::Owner).count(),
        1,
        "exactly one owner at a time"
    );
}

#[test]
fn push_chat_zalgo_prefix_keeps_trailing_text() {
    let mut state = sim::new_game(5, 12);
    sim::player_joined(&mut state, 7, "Zara");

    // A zalgo-heavy prefix must not consume the whole length budget and discard
    // the sender's real trailing message (combining-cap runs before the cap).
    let mut text = String::from("a");
    for _ in 0..199 {
        text.push('\u{0301}');
    }
    text.push_str(" HELP");
    sim::push_chat(&mut state, 7, &text);

    let stored = &state.chat[0].text;
    assert!(
        stored.contains("HELP"),
        "trailing text must survive the zalgo cap: {stored:?}"
    );
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

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x20D0..=0x20FF
        | 0xFE20..=0xFE2F
    )
}
