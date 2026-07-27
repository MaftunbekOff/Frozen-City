use crate::rng::Rng;
use crate::types::*;
use super::*;

const NAMES: [&str; 24] = [
    "Anna", "Boris", "Clara", "Dmitri", "Edda", "Finn", "Greta", "Henrik", "Ilya", "Jonas",
    "Katya", "Lena", "Mikkel", "Nadia", "Oskar", "Petra", "Ravi", "Sonja", "Tomas", "Ulla",
    "Viktor", "Wanda", "Yuri", "Zoya",
];

/// The city starts with just its leader — everyone else arrives once the
/// furnace is actually lit (`tick.rs`'s morning-arrivals check gates on
/// `furnace_lit`), echoing the "one survivor chops wood and lights the
/// first fire" opening.
const START_SURVIVORS: usize = 1;
const START_WOOD: f32 = 60.0;
const START_COAL: f32 = 40.0;
const START_FOOD: f32 = 25.0;
/// V0.11: same "a few weeks' buffer for the lone starting survivor" ratio
/// `START_FOOD` uses relative to `FOOD_PER_SURVIVOR_DAY` (~21 days), applied
/// to `WATER_PER_SURVIVOR_DAY` — without this, a fresh colony has no way to
/// get any water at all before thirst turns lethal (well under a day), long
/// before there's been any real chance to build and staff a Well.
const START_WATER: f32 = 30.0;

pub fn new_game(seed: u64, win_days: u32) -> GameState {
    let mut rng = Rng::new(seed);
    let mut tiles = vec![
        Tile { terrain: Terrain::Snow, deposit: 0 };
        MAP_W * MAP_H
    ];

    // Forest blobs.
    for _ in 0..14 {
        if let Some((cx, cy)) = pick_blob_center(&mut rng, 9) {
            let steps = 25 + rng.below(21);
            blob_walk(&mut tiles, &mut rng, cx, cy, steps, Terrain::Forest, 40, 80, 6);
        }
    }
    // Coal deposits.
    for _ in 0..6 {
        if let Some((cx, cy)) = pick_blob_center(&mut rng, 12) {
            let steps = 8 + rng.below(9);
            blob_walk(&mut tiles, &mut rng, cx, cy, steps, Terrain::Coal, 250, 500, 8);
        }
    }
    // Scattered lone trees.
    for _ in 0..80 {
        let x = rng.below(MAP_W as u32) as u8;
        let y = rng.below(MAP_H as u32) as u8;
        let idx = tile_index(x, y);
        if tiles[idx].terrain == Terrain::Snow && GameState::dist_to_furnace(x, y) >= 6.0 {
            tiles[idx] = Tile {
                terrain: Terrain::Forest,
                deposit: 25 + rng.below(26) as u16,
            };
        }
    }

    let mut next_id: u32 = 1;
    let mut survivors = Vec::new();
    for _ in 0..START_SURVIVORS {
        survivors.push(new_survivor(&mut rng, &mut next_id));
    }
    // The lone starting survivor opens the game as leader — whatever their
    // rolled profession, they're the one who'll build and light the furnace,
    // which only reads as coherent if `survivor_contribution`'s
    // leader-is-universal bypass already applies to them from tick 0. They
    // stay leader until `SetLeader` names someone else.
    let starting_leader = survivors.first().map(|s| s.id);

    let furnace = Building {
        id: 0,
        kind: BuildingKind::Furnace,
        x: FURNACE_X,
        y: FURNACE_Y,
        workers: 0,
        progress: 0.0,
        owner: None,
        owner_account: None,
        // Boshlang'ich pech qurilishsiz emas — yetakchi uni tiklashi kerak
        // (`AssignSurvivor` orqali, xuddi boshqa qurilish maydonchalari
        // kabi), lekin usta-kun emas — `build_left` bu yerda "qolgan
        // o'tinlar soni" (`FURNACE_LOGS_NEEDED`): har biri haqiqiy
        // chopib-ko'tarib-kelish sayohati (`tick.rs`ning pech-qurilish
        // blokiga qarang). Tugagach `tick.rs` `furnace_lit`/
        // `furnace_level`ni o'rnatadi va `SetFurnaceLevel` ochiladi
        // (`command.rs`).
        level: 1,
        build_left: FURNACE_LOGS_NEEDED as f32,
        facing: 0,
    };

    // The Tunnel to the Global World — present (sealed-looking) from the
    // very start, long before it's unlocked; `level`/`build_left` are inert
    // for it (its real excavation state is `GameState.tunnel`, below). Takes
    // the next id off the shared counter, same as any other placed building
    // (unlike the furnace's reserved `id: 0` — this runs after the starting
    // survivor(s) already claimed id 1).
    let tunnel_building = Building {
        id: next_id,
        kind: BuildingKind::Tunnel,
        x: TUNNEL_X,
        y: TUNNEL_Y,
        workers: 0,
        progress: 0.0,
        owner: None,
        owner_account: None,
        level: 1,
        build_left: 0.0,
        facing: 0,
    };
    next_id += 1;

    let mut state = GameState {
        // Start mid-morning of day 1.
        tick: ARRIVAL_TICK,
        win_days,
        tiles,
        buildings: vec![furnace, tunnel_building],
        survivors,
        stock: Stockpile {
            wood: START_WOOD,
            coal: START_COAL,
            food: START_FOOD,
            fur: 0.0,
            cloth: 0.0,
            water: START_WATER,
            gold: 0.0,
        },
        // Unset until the leader finishes building the furnace (see the
        // `Building` above) — `tick.rs`'s construction-complete arm sets
        // both once `build_left` reaches 0.
        furnace_level: 0,
        furnace_lit: false,
        cold_snap: false,
        players: Vec::new(),
        phase: GamePhase::Running,
        events: Vec::new(),
        total_events: 0,
        chat: Vec::new(),
        total_chat: 0,
        pings: Vec::new(),
        missions: vec![
            Mission { kind: MissionKind::BuildTents(2),     reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::Population(10),    reward_wood: 0,  reward_coal: 0, reward_food: 20, done: false },
            Mission { kind: MissionKind::Sawmills(1),       reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::StockpileCoal(60), reward_wood: 20, reward_coal: 0, reward_food: 0,  done: false },
            Mission { kind: MissionKind::SurviveDays(4),    reward_wood: 0,  reward_coal: 0, reward_food: 30, done: false },
        ],
        tunnel: TunnelState::default(),
        graduated: false,
        central: false,
        techs: Vec::new(),
        disease_until: 0,
        blizzard_until: 0,
        pending_event: None,
        event_rng: Rng::new(seed.rotate_left(21) ^ 0x00E0_0DE0_1234_5678).0,
        owner_id: None,
        next_id,
        rng: rng.0,
        central_ledger: Vec::new(),
        leader: starting_leader,
        mourning_until: 0,
        morale: MORALE_START,
        pending_migrant: None,
        wildlife: Wildlife { deer: DEER_START, rabbit: RABBIT_START },
        corpses: Vec::new(),
        graves: Vec::new(),
        livestock: Livestock { cow: COW_START, sheep: SHEEP_START },
        pending_caravan: None,
        illness_rng: Rng::new(seed.rotate_left(37) ^ ILLNESS_RNG_SEED).0,
    };
    push_event(
        &mut state,
        format!(
            "One survivor remains. Build the furnace to call the others home — survive until day {}.",
            win_days
        ),
    );
    state
}

/// The one shared central world — the Global World on the far side of the
/// Tunnel. Same procedural map, but it starts with no population (settlers
/// only ever arrive through the Tunnel, owned by their account) and no
/// personal-world progression: no missions, so the Tunnel can never unlock
/// (`all_missions_done` is false on an empty list), and `tick` skips
/// hunger/death/win/lose/events/arrivals for `central` worlds.
pub fn new_game_central(seed: u64) -> GameState {
    let mut state = new_game(seed, DEFAULT_WIN_DAYS);
    state.central = true;
    state.survivors.clear();
    state.leader = None;
    state.missions.clear();
    state.events.clear();
    state.total_events = 0;
    push_event(
        &mut state,
        "The Global World. Settlers arrive through the Tunnel.",
    );
    state
}

/// Population `new_game_bootstrapped` tops up to — the old `START_SURVIVORS`
/// count, kept only for that helper so unrelated tests don't have to care
/// about the furnace-bootstrap opening.
const BOOTSTRAPPED_SURVIVORS: usize = 8;

/// Test/tooling convenience: `new_game` fast-forwarded past the "one leader
/// builds the furnace" opening — furnace lit at level 1 and
/// [`BOOTSTRAPPED_SURVIVORS`] present, matching what most mechanic tests
/// actually want to exercise (production, healing, XP, assignment, ...)
/// without re-testing the bootstrap sequence itself (see
/// `tests/furnace_bootstrap_tests.rs` for that).
pub fn new_game_bootstrapped(seed: u64, win_days: u32) -> GameState {
    let mut state = new_game(seed, win_days);
    if let Some(f) = state.buildings.iter_mut().find(|b| b.kind == BuildingKind::Furnace) {
        f.build_left = 0.0;
    }
    state.furnace_lit = true;
    state.furnace_level = 1;
    let mut rng = Rng(state.rng);
    while state.survivors.len() < BOOTSTRAPPED_SURVIVORS {
        let s = new_survivor(&mut rng, &mut state.next_id);
        state.survivors.push(s);
    }
    state.rng = rng.0;
    state
}

fn pick_blob_center(rng: &mut Rng, min_dist: i32) -> Option<(u8, u8)> {
    for _ in 0..40 {
        let x = rng.below(MAP_W as u32) as u8;
        let y = rng.below(MAP_H as u32) as u8;
        if GameState::dist_to_furnace(x, y) >= min_dist as f32 {
            return Some((x, y));
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn blob_walk(
    tiles: &mut [Tile],
    rng: &mut Rng,
    cx: u8,
    cy: u8,
    steps: u32,
    terrain: Terrain,
    dep_lo: u32,
    dep_hi: u32,
    keep_clear: i32,
) {
    let mut x = cx as i32;
    let mut y = cy as i32;
    for _ in 0..steps {
        if in_bounds(x, y) {
            let (ux, uy) = (x as u8, y as u8);
            let idx = tile_index(ux, uy);
            if tiles[idx].terrain == Terrain::Snow
                && GameState::dist_to_furnace(ux, uy) >= keep_clear as f32
            {
                tiles[idx] = Tile {
                    terrain,
                    deposit: (dep_lo + rng.below(dep_hi - dep_lo + 1)) as u16,
                };
            }
        }
        x = (x + rng.range(-1, 1)).clamp(0, MAP_W as i32 - 1);
        y = (y + rng.range(-1, 1)).clamp(0, MAP_H as i32 - 1);
    }
}

pub(crate) fn new_survivor(rng: &mut Rng, next_id: &mut u32) -> Survivor {
    let id = *next_id;
    *next_id += 1;
    let (x, y) = GameState::spawn_position(id);
    Survivor {
        id,
        name: NAMES[rng.below(NAMES.len() as u32) as usize].to_string(),
        hp: 85.0 + rng.below(16) as f32,
        hunger: 20.0 + rng.below(21) as f32,
        thirst: 15.0 + rng.below(21) as f32,
        assigned_building: None,
        owner: None,
        x,
        y,
        move_target: None,
        // Drawn from the same sim RNG stream as name/hp/hunger — deterministic
        // per-seed like everything else `new_survivor` sets, and distinct
        // from `Profession::from_id_hash` (which only exists for migrated
        // saves with no RNG stream to draw from).
        profession: Profession::ALL[rng.below(Profession::ALL.len() as u32) as usize],
        xp: 0.0,
        trained_kind: None,
        chop_target: None,
        carrying_wood: false,
        bury_target: None,
        // V0.17: everyone starts rested and healthy — a newcomer walking in
        // out of the cold is the one moment the colony's own conditions
        // haven't touched them yet.
        fatigue: 0.0,
        sick_left: 0.0,
    }
}

/// Like `find_forest_tile` but consumes: removes one unit of wood from the
/// nearest forest tile within `r` of (cx, cy) — the Sawmill's instant,
/// no-survivor-position abstraction.
pub(crate) fn take_forest_unit(tiles: &mut [Tile], cx: u8, cy: u8, r: i32) -> bool {
    let mut best: Option<(i32, usize)> = None;
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            if !in_bounds(x, y) {
                continue;
            }
            let idx = tile_index(x as u8, y as u8);
            let t = &tiles[idx];
            if t.terrain == Terrain::Forest && t.deposit > 0 {
                let d = dx.abs().max(dy.abs());
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, idx));
                }
            }
        }
    }
    if let Some((_, idx)) = best {
        tiles[idx].deposit -= 1;
        if tiles[idx].deposit == 0 {
            tiles[idx].terrain = Terrain::Snow;
        }
        true
    } else {
        false
    }
}

/// Nearest forest tile with wood left within `r` of (cx, cy), without
/// consuming it — for a survivor to walk toward before chopping it on
/// arrival (the Furnace's chop-and-carry construction cycle, `tick.rs`).
/// Same nearest-tile search as `take_forest_unit`, just read-only.
pub(crate) fn find_forest_tile(tiles: &[Tile], cx: u8, cy: u8, r: i32) -> Option<(u8, u8)> {
    let mut best: Option<(i32, (u8, u8))> = None;
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx as i32 + dx, cy as i32 + dy);
            if !in_bounds(x, y) {
                continue;
            }
            let (ux, uy) = (x as u8, y as u8);
            let t = &tiles[tile_index(ux, uy)];
            if t.terrain == Terrain::Forest && t.deposit > 0 {
                let d = dx.abs().max(dy.abs());
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, (ux, uy)));
                }
            }
        }
    }
    best.map(|(_, pos)| pos)
}
