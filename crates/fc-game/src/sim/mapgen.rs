use crate::rng::Rng;
use crate::types::*;
use super::*;

const NAMES: [&str; 24] = [
    "Anna", "Boris", "Clara", "Dmitri", "Edda", "Finn", "Greta", "Henrik", "Ilya", "Jonas",
    "Katya", "Lena", "Mikkel", "Nadia", "Oskar", "Petra", "Ravi", "Sonja", "Tomas", "Ulla",
    "Viktor", "Wanda", "Yuri", "Zoya",
];

const START_SURVIVORS: usize = 8;
const START_WOOD: f32 = 60.0;
const START_COAL: f32 = 40.0;
const START_FOOD: f32 = 25.0;

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

    let furnace = Building {
        id: 0,
        kind: BuildingKind::Furnace,
        x: FURNACE_X,
        y: FURNACE_Y,
        workers: 0,
        progress: 0.0,
        owner: None,
        owner_account: None,
        // Boshlang'ich pech tayyor holda tug'iladi (qurilishsiz).
        level: 1,
        build_left: 0.0,
    };

    let mut state = GameState {
        // Start mid-morning of day 1.
        tick: ARRIVAL_TICK,
        win_days,
        tiles,
        buildings: vec![furnace],
        survivors,
        stock: Stockpile {
            wood: START_WOOD,
            coal: START_COAL,
            food: START_FOOD,
        },
        furnace_level: 1,
        furnace_lit: true,
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
        guest_perm: GuestPermission::Build,
        owner_id: None,
        next_id,
        rng: rng.0,
        central_ledger: Vec::new(),
        leader: None,
        mourning_until: 0,
        morale: MORALE_START,
    };
    push_event(
        &mut state,
        format!(
            "The last furnace is lit. Survive until day {}.",
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
    state.missions.clear();
    state.events.clear();
    state.total_events = 0;
    push_event(
        &mut state,
        "The Global World. Settlers arrive through the Tunnel.",
    );
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
    }
}

/// Remove one unit of wood from the nearest forest tile within `r` of (cx, cy).
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
