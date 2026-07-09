//! Procedural audio: every sound the game makes is synthesized as raw PCM at
//! startup and handed to `bevy_audio` as an in-memory WAV — the project ships
//! no asset files, audio included. Covers the three audible moments the
//! client cares about: placing a building, a new world event landing, and
//! the blizzard wind bed.

use std::f32::consts::TAU;

use bevy::audio::Volume;
use bevy::prelude::*;

use super::{GameView, Screen};

/// Sample rate for every synthesized clip. Modest on purpose: these are tiny,
/// simple waveforms, so CD quality would just be wasted memory.
const SAMPLE_RATE: u32 = 22_050;
/// Ceiling every generator scales into, well under full scale (`i16::MAX`)
/// so nothing clips or startles even if several one-shots overlap.
const PEAK: f32 = 0.25 * i16::MAX as f32;
/// Extra headroom for the wind bed specifically — it is a constant
/// background loop, so it needs to sit well below the one-shot SFX.
const WIND_GAIN: f32 = 0.5;
/// Wind volume once a blizzard is in full swing (see [`blizzard_volume`]).
const BLIZZARD_WIND_VOLUME: f32 = 0.6;

// ------------------------------------------------------------- WAV encoding

/// Wrap mono 16-bit PCM samples in a minimal RIFF/WAVE container — the
/// entire on-disk format `bevy_audio`'s decoder needs, built with no
/// external crate.
fn wav_bytes(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = samples.len() * 2;
    let byte_rate = sample_rate * 2; // mono, 16-bit -> 2 bytes/sample frame
    let mut out = Vec::with_capacity(44 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size (PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // format tag: PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // channels: mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align: bytes/frame
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// Float sample (roughly `[-1, 1]`) -> clamped, pre-scaled 16-bit PCM sample.
fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * PEAK) as i16
}

// ------------------------------------------------------------- generators

/// Soft low "thunk" for placing a building (~0.12s): a ~180Hz tone that
/// decays almost immediately so repeated builds don't smear into noise.
fn gen_place() -> Vec<i16> {
    let n = (SAMPLE_RATE as f32 * 0.12) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let env = (-t * 26.0).exp(); // fast exponential decay
            to_i16((TAU * 180.0 * t).sin() * env)
        })
        .collect()
}

/// Two-note "event chime" (~0.35s total): a soft 660Hz -> 880Hz ding-ding,
/// each note with a quick attack and gentle decay so it reads as a bell
/// rather than a buzzer.
fn gen_event() -> Vec<i16> {
    const NOTES: [f32; 2] = [660.0, 880.0];
    let n = (SAMPLE_RATE as f32 * 0.175) as usize; // per note; 2 notes -> ~0.35s
    let mut out = Vec::with_capacity(n * NOTES.len());
    for freq in NOTES {
        for i in 0..n {
            let t = i as f32 / SAMPLE_RATE as f32;
            let attack = (t / 0.008).min(1.0);
            let decay = (-t * 8.0).exp();
            out.push(to_i16((TAU * freq * t).sin() * attack * decay));
        }
    }
    out
}

/// Small deterministic linear-congruential generator, so the wind noise is
/// reproducible without pulling in the `rand` crate.
struct Lcg(u64);

impl Lcg {
    /// Next pseudo-random value in `[-1.0, 1.0)`.
    fn next_signed(&mut self) -> f32 {
        // Numerical Recipes' 64-bit LCG constants.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let bits = (self.0 >> 40) as u32; // top 24 bits: the higher-quality ones
        (bits as f32 / 16_777_216.0) * 2.0 - 1.0
    }
}

/// ~1.0s loopable wind bed for blizzards: value noise (LCG white noise
/// passed through a circular moving-average low-pass) plus a slow gust
/// swell. Both stages are built to be exactly periodic over the buffer
/// length, so the loop has no audible seam. Deliberately quiet —
/// [`blizzard_volume`] scales it further at runtime.
fn gen_wind() -> Vec<i16> {
    let n = SAMPLE_RATE as usize; // exactly 1.0s
    let mut rng = Lcg(0x9E37_79B9_7F4A_7C15);
    let raw: Vec<f32> = (0..n).map(|_| rng.next_signed()).collect();

    // Circular moving-average low-pass: treating `raw` as one period of a
    // periodic signal makes the smoothed result itself perfectly periodic,
    // so it loops without a click at the wrap point.
    let half_win = 90usize; // ~8ms either side at 22.05kHz -> soft gusty hiss
    let win = 2 * half_win + 1;
    let mut smoothed = vec![0.0f32; n];
    for (i, out) in smoothed.iter_mut().enumerate() {
        let mut sum = 0.0f32;
        for k in 0..win {
            sum += raw[(i + n - half_win + k) % n];
        }
        *out = sum / win as f32;
    }

    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32; // one full loop, 0..1
            // Slow gust swell; an integer cycle count keeps the tail sample
            // matching the head sample, so this stays seamless too.
            let gust = 0.55 + 0.35 * (TAU * 3.0 * t).sin();
            to_i16(smoothed[i] * gust * WIND_GAIN)
        })
        .collect()
}

// --------------------------------------------------------------- resources

/// Handles to the three synthesized clips, built once at startup.
#[derive(Resource)]
struct Sfx {
    place: Handle<AudioSource>,
    event: Handle<AudioSource>,
    wind: Handle<AudioSource>,
}

/// Marks the single looping wind-bed entity for the current game session.
#[derive(Component)]
struct WindSound;

fn setup_sfx(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
    let mut clip = |samples: Vec<i16>| -> Handle<AudioSource> {
        sources.add(AudioSource { bytes: wav_bytes(&samples, SAMPLE_RATE).into() })
    };
    let sfx = Sfx { place: clip(gen_place()), event: clip(gen_event()), wind: clip(gen_wind()) };
    commands.insert_resource(sfx);
}

/// Spawn the blizzard wind loop silent; [`blizzard_volume`] eases it in and
/// out as blizzards start and end.
fn spawn_wind(mut commands: Commands, sfx: Res<Sfx>) {
    commands.spawn((
        AudioPlayer(sfx.wind.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.0)),
        WindSound,
        DespawnOnExit(Screen::Game),
    ));
}

// ----------------------------------------------------------------- systems

/// Ease the looping wind's volume toward its target instead of snapping, so
/// a blizzard starting or ending fades rather than clicks.
fn blizzard_volume(
    view: Res<GameView>,
    time: Res<Time>,
    mut sinks: Query<&mut AudioSink, With<WindSound>>,
) {
    let Ok(mut sink) = sinks.single_mut() else { return };
    let target = match view.state.as_ref() {
        Some(state) if state.blizzard_active() => BLIZZARD_WIND_VOLUME,
        _ => 0.0,
    };
    let current = sink.volume().to_linear();
    let eased = current + (target - current) * (3.0 * time.delta_secs()).min(1.0);
    sink.set_volume(Volume::Linear(eased));
}

/// Fires a one-shot when the world changes: a soft thunk each time the
/// building count grows, a two-note chime each time a new event lands.
/// `prev` only ever compares against the previous frame, so the very first
/// frame just primes it rather than firing a burst on load.
fn sfx_on_change(
    mut commands: Commands,
    view: Res<GameView>,
    sfx: Res<Sfx>,
    mut prev: Local<Option<(usize, u64)>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    let buildings = state.buildings.len();
    let events = state.total_events;

    let Some((prev_buildings, prev_events)) = *prev else {
        *prev = Some((buildings, events));
        return;
    };

    // A lower count than last frame means a new session started (leaving a
    // game resets the mirrored snapshot) rather than something shrinking —
    // resync quietly instead of risking a burst of stale-looking sounds.
    if buildings < prev_buildings || events < prev_events {
        *prev = Some((buildings, events));
        return;
    }

    if buildings > prev_buildings {
        commands.spawn((AudioPlayer(sfx.place.clone()), PlaybackSettings::DESPAWN));
    }
    if events > prev_events {
        commands.spawn((AudioPlayer(sfx.event.clone()), PlaybackSettings::DESPAWN));
    }
    *prev = Some((buildings, events));
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup_sfx)
        .add_systems(OnEnter(Screen::Game), spawn_wind)
        .add_systems(Update, (blizzard_volume, sfx_on_change).run_if(in_state(Screen::Game)));
}
