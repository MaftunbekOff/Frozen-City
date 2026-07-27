//! V0.18: the expedition panel — pick a site, pick a party, send them out,
//! watch the road, call them back.
//!
//! Modelled on [`super::research`]: a modal over a scrim, toggled by a key and
//! by a floating HUD button (mobile has no keyboard, mirrors `social`'s
//! `SocialHudBtn`), reading `GameView` and sending `PlayerCommand`s. The
//! server is authoritative — this only mirrors `GameState.expeditions` and
//! greys what `GameState::can_launch_expedition` refuses. `can_launch_
//! expedition`'s Err text is sim-authored plain English and is shown as-is
//! (see `i18n_expedition`'s doc comment) — everything else is translated.

use bevy::prelude::*;

use frozen_city::game::types::{
    ExpeditionSite, GameState, LifeStage, PlayerCommand, Survivor, EXPEDITION_MAX_PARTY,
    EXPEDITION_MIN_PARTY, EXPEDITION_MIN_STAY_HOME, TICKS_PER_DAY,
};
use frozen_city::net::protocol::ClientMsg;

use super::chat::ChatState;
use super::i18n::Lang;
use super::i18n_expedition;
use super::roster;
use super::theme::{self, FormFactor};
use super::ui::UiBlocker;
use super::{GameView, NetConn, Screen};

/// Visible party-candidate rows at once — mirrors `roster.rs`'s `ROSTER_ROWS`
/// (population can reach `MAX_POPULATION`, which doesn't fit on screen).
const PARTY_ROWS: usize = 20;

/// Whether the expedition modal is open (also gates world/camera input, same
/// as `ResearchOpen`).
#[derive(Resource, Default)]
pub struct ExpeditionOpen(pub bool);

/// The site currently highlighted in the picker. Defaults to the first (and
/// gentlest) site so there's always a valid selection to preview/launch —
/// nothing forces the player to pick a site before a party, and vice versa.
#[derive(Resource)]
struct SelectedSite(ExpeditionSite);

impl Default for SelectedSite {
    fn default() -> Self {
        SelectedSite(ExpeditionSite::FrozenWoods)
    }
}

/// Survivor ids currently queued for the next launch. Pruned of anyone who's
/// died or otherwise left the roster every frame (`update_party_rows`),
/// mirroring `roster`'s `SurvivorSelection` cleanup, and cleared once a
/// launch actually goes out.
#[derive(Resource, Default)]
struct SelectedParty(Vec<u32>);

#[derive(Component)]
struct ExpeditionRoot;

#[derive(Component)]
struct TitleText;

#[derive(Component)]
struct ActiveSection;

#[derive(Component)]
struct PickerSection;

#[derive(Component)]
struct ActiveTitleText;

#[derive(Component)]
struct ActiveStatusText;

#[derive(Component)]
struct ActiveProgressFill;

#[derive(Component)]
struct ActivePartyText;

#[derive(Component)]
struct RecallBtn;

#[derive(Component)]
struct RecallLabel;

#[derive(Component)]
struct SiteBtn(ExpeditionSite);

#[derive(Component)]
struct SiteBtnBg(ExpeditionSite);

#[derive(Component)]
struct SiteNameText(ExpeditionSite);

#[derive(Component)]
struct SiteInfoText(ExpeditionSite);

#[derive(Component)]
struct PartyHeaderText;

#[derive(Component)]
struct PartyRow(usize);

/// Clicking toggles `survivor` in/out of `SelectedParty` (or does nothing
/// while it's `None`, an empty row). Kept in sync with the current sorted
/// candidate list every frame in `update_party_rows`, mirroring
/// `roster::RosterRowBtn`.
#[derive(Component)]
struct PartyRowBtn {
    row: usize,
    survivor: Option<u32>,
}

#[derive(Component)]
struct PartyNameText(usize);

#[derive(Component)]
struct PartyTagText(usize);

#[derive(Component)]
struct MoreText;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct LaunchBtn;

#[derive(Component)]
struct LaunchLabel;

/// Floating HUD button (mobile has no `E` key) — mirrors `social`'s
/// `SocialHudBtn` in both role and placement convention (spawned by this
/// module, not `ui::spawn_hud`, so it survives independently of that
/// module's layout).
#[derive(Component)]
struct ExpeditionHudBtn;

pub fn plugin(app: &mut App) {
    app.init_resource::<ExpeditionOpen>()
        .init_resource::<SelectedSite>()
        .init_resource::<SelectedParty>()
        .add_systems(OnEnter(Screen::Game), (spawn_expedition, spawn_hud_button))
        .add_systems(
            Update,
            (
                toggle_expedition,
                expedition_hud_button,
                update_shell,
                update_active_section,
                update_site_cards,
                update_party_rows,
                update_status_and_launch,
                site_buttons,
                party_row_buttons,
                launch_button,
                recall_button,
            )
                .run_if(in_state(Screen::Game)),
        );
}

fn spawn_expedition(mut commands: Commands, ff: Res<FormFactor>) {
    let ff = *ff;
    commands
        .spawn((theme::scrim(ff), UiBlocker, ExpeditionRoot, DespawnOnExit(Screen::Game)))
        .with_children(|p| {
            p.spawn(theme::modal_panel(ff)).with_children(|panel| {
                panel.spawn((theme::title(""), TitleText));

                // --- Active-expedition section: shown while a party is away. ---
                panel
                    .spawn((
                        Node {
                            display: Display::None,
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_SM),
                            ..default()
                        },
                        ActiveSection,
                    ))
                    .with_children(|a| {
                        a.spawn((theme::text("", theme::FS_SECTION, theme::ACCENT_ICE), ActiveTitleText));
                        a.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), ActiveStatusText));
                        a.spawn(theme::stat_bar_track(Val::Percent(100.0), 10.0)).with_children(|track| {
                            track.spawn((
                                Node {
                                    width: Val::Percent(0.0),
                                    height: Val::Percent(100.0),
                                    border_radius: BorderRadius::MAX,
                                    ..default()
                                },
                                BackgroundColor(theme::ACCENT_WARM),
                                ActiveProgressFill,
                            ));
                        });
                        a.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), ActivePartyText));
                        // No `theme::button()` (bakes in `BaseColor`) and no
                        // `BaseColor` of its own: this button's color is owned
                        // entirely by `update_active_section` (danger/dim by
                        // `recalled`), same reasoning as `research.rs`'s
                        // `ResearchBtn`/`missions.rs`'s `TunnelInvestBtn`.
                        a.spawn((
                            Button,
                            Node {
                                width: Val::Percent(100.0),
                                height: Val::Px(ff.btn_h()),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                ..default()
                            },
                            BackgroundColor(theme::BTN_DANGER),
                            RecallBtn,
                        ))
                        .with_children(|b| {
                            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), RecallLabel));
                        });
                    });

                // --- Picker section: shown while no party is out. ---
                panel
                    .spawn((
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_SM),
                            ..default()
                        },
                        PickerSection,
                    ))
                    .with_children(|picker| {
                        for site in ExpeditionSite::ALL {
                            picker
                                .spawn((
                                    Button,
                                    Node {
                                        flex_direction: FlexDirection::Column,
                                        align_items: AlignItems::FlexStart,
                                        row_gap: Val::Px(theme::SP_XS),
                                        padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_SM)),
                                        border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                        ..default()
                                    },
                                    // No BaseColor: `update_site_cards` owns this
                                    // button's color entirely (selected/dim), same
                                    // reasoning as `research.rs`'s `ResearchBtn`.
                                    BackgroundColor(theme::BTN),
                                    SiteBtn(site),
                                    SiteBtnBg(site),
                                ))
                                .with_children(|row| {
                                    row.spawn((theme::text("", theme::FS_BODY, theme::TEXT_PRIMARY), SiteNameText(site)));
                                    row.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_MUTED), SiteInfoText(site)));
                                });
                        }

                        picker.spawn(theme::divider());
                        picker.spawn((theme::text("", theme::FS_SMALL, theme::ACCENT_ICE), PartyHeaderText));
                        for i in 0..PARTY_ROWS {
                            picker
                                .spawn((
                                    Node {
                                        display: Display::None,
                                        align_items: AlignItems::Center,
                                        column_gap: Val::Px(theme::SP_SM),
                                        ..default()
                                    },
                                    PartyRow(i),
                                ))
                                .with_children(|row| {
                                    row.spawn((
                                        Button,
                                        Node {
                                            flex_grow: 1.0,
                                            flex_shrink: 1.0,
                                            min_width: Val::Px(0.0),
                                            align_items: AlignItems::Center,
                                            justify_content: JustifyContent::SpaceBetween,
                                            padding: UiRect::axes(Val::Px(theme::SP_SM), Val::Px(theme::SP_XS)),
                                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                            ..default()
                                        },
                                        BackgroundColor(theme::BG_SECTION),
                                        PartyRowBtn { row: i, survivor: None },
                                    ))
                                    .with_children(|btn| {
                                        btn.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), PartyNameText(i)));
                                        btn.spawn((theme::text("", theme::FS_MICRO, theme::TEXT_MUTED), PartyTagText(i)));
                                    });
                                });
                        }
                        picker.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), MoreText));

                        picker.spawn(theme::divider());
                        picker.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_MUTED), StatusText));
                        picker
                            .spawn((
                                Button,
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Px(ff.btn_h()),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                // No BaseColor: `update_status_and_launch` owns
                                // this button's color (ready/dim).
                                BackgroundColor(theme::BTN_DIM),
                                LaunchBtn,
                            ))
                            .with_children(|b| {
                                b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), LaunchLabel));
                            });
                    });
            });
        });
}

/// Mirrors `social::spawn::spawn_hud_button` exactly: a separate root so it
/// survives independently of `ui::spawn_hud`'s layout. Sits directly below
/// the Friends button (`left:206/12, top:78/196`, 90x28) in the same left-
/// side column rather than fighting it for the same row.
fn spawn_hud_button(mut commands: Commands, ff: Res<FormFactor>) {
    commands
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(if ff.compact() { 12.0 } else { 206.0 }),
                top: Val::Px(if ff.compact() { 230.0 } else { 112.0 }),
                width: Val::Px(90.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                ..default()
            },
            BackgroundColor(theme::BTN),
            theme::BaseColor(theme::BTN),
            ExpeditionHudBtn,
            DespawnOnExit(Screen::Game),
        ))
        .with_children(|b| {
            b.spawn((theme::text("", theme::FS_SMALL, theme::TEXT_PRIMARY), LabelFor::HudBtn));
        });
}

/// The HUD button's own label needs a language-driven refresh too, just like
/// every other text in this panel; a one-variant marker is simpler than a
/// second freestanding component for a single string.
#[derive(Component)]
enum LabelFor {
    HudBtn,
}

fn toggle_expedition(keys: Res<ButtonInput<KeyCode>>, chat: Res<ChatState>, mut open: ResMut<ExpeditionOpen>) {
    if !chat.active && keys.just_pressed(KeyCode::KeyE) {
        open.0 = !open.0;
    }
    if open.0 && keys.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn expedition_hud_button(
    q: Query<&Interaction, (With<ExpeditionHudBtn>, Changed<Interaction>)>,
    mut open: ResMut<ExpeditionOpen>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            open.0 = !open.0;
        }
    }
}

/// Root visibility, title, HUD button label, and the active-vs-picker section
/// swap — `state.expeditions` is non-empty exactly when `EXPEDITION_MAX_
/// ACTIVE` (1) is in use, so there is never a choice to preview alongside an
/// active trip.
fn update_shell(
    open: Res<ExpeditionOpen>,
    view: Res<GameView>,
    lang: Res<Lang>,
    mut root: Query<&mut Node, (With<ExpeditionRoot>, Without<ActiveSection>, Without<PickerSection>)>,
    mut title: Query<&mut Text, With<TitleText>>,
    mut active: Query<&mut Node, (With<ActiveSection>, Without<ExpeditionRoot>, Without<PickerSection>)>,
    mut picker: Query<&mut Node, (With<PickerSection>, Without<ExpeditionRoot>, Without<ActiveSection>)>,
    mut hud_label: Query<&mut Text, (With<LabelFor>, Without<TitleText>)>,
) {
    let lang = *lang;
    let display = if open.0 { Display::Flex } else { Display::None };
    for mut node in &mut root {
        if node.display != display {
            node.display = display;
        }
    }
    if let Ok(mut t) = title.single_mut() {
        let new = format!("{}   {}", i18n_expedition::title(lang), i18n_expedition::hint(lang));
        if t.0 != new {
            t.0 = new;
        }
    }
    for mut t in &mut hud_label {
        if t.0 != i18n_expedition::hud_btn(lang) {
            t.0 = i18n_expedition::hud_btn(lang).to_string();
        }
    }

    let Some(state) = view.state.as_ref() else { return };
    let has_active = !state.expeditions.is_empty();
    let active_display = if has_active { Display::Flex } else { Display::None };
    let picker_display = if has_active { Display::None } else { Display::Flex };
    for mut node in &mut active {
        if node.display != active_display {
            node.display = active_display;
        }
    }
    for mut node in &mut picker {
        if node.display != picker_display {
            node.display = picker_display;
        }
    }
}

fn update_active_section(
    view: Res<GameView>,
    lang: Res<Lang>,
    #[allow(clippy::type_complexity)]
    mut active_title: Query<&mut Text, (With<ActiveTitleText>, Without<ActiveStatusText>, Without<ActivePartyText>, Without<RecallLabel>)>,
    #[allow(clippy::type_complexity)]
    mut active_status: Query<&mut Text, (With<ActiveStatusText>, Without<ActiveTitleText>, Without<ActivePartyText>, Without<RecallLabel>)>,
    #[allow(clippy::type_complexity)]
    mut active_party: Query<&mut Text, (With<ActivePartyText>, Without<ActiveTitleText>, Without<ActiveStatusText>, Without<RecallLabel>)>,
    #[allow(clippy::type_complexity)]
    mut recall_label: Query<&mut Text, (With<RecallLabel>, Without<ActiveTitleText>, Without<ActiveStatusText>, Without<ActivePartyText>)>,
    mut progress_fill: Query<&mut Node, With<ActiveProgressFill>>,
    mut recall_bg: Query<&mut BackgroundColor, With<RecallBtn>>,
) {
    let lang = *lang;
    if let Ok(mut t) = recall_label.single_mut() {
        if t.0 != i18n_expedition::recall_btn(lang) {
            t.0 = i18n_expedition::recall_btn(lang).to_string();
        }
    }

    let Some(state) = view.state.as_ref() else { return };
    let Some(e) = state.expeditions.first() else { return };
    let now = state.tick;

    if let Ok(mut t) = active_title.single_mut() {
        let new = i18n_expedition::active_title(i18n_expedition::site_name(e.site, lang), lang);
        if t.0 != new {
            t.0 = new;
        }
    }
    if let Ok(mut t) = active_status.single_mut() {
        let new = if e.recalled {
            i18n_expedition::active_recalled(lang).to_string()
        } else {
            let days_left = e.return_tick.saturating_sub(now) as f32 / TICKS_PER_DAY as f32;
            i18n_expedition::active_days_left(days_left, lang)
        };
        if t.0 != new {
            t.0 = new;
        }
    }
    if let Ok(mut t) = active_party.single_mut() {
        let names = e.party.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
        let new = i18n_expedition::active_party_line(&names, lang);
        if t.0 != new {
            t.0 = new;
        }
    }
    if let Ok(mut node) = progress_fill.single_mut() {
        let w = Val::Percent(e.progress(now) * 100.0);
        if node.width != w {
            node.width = w;
        }
    }
    if let Ok(mut bg) = recall_bg.single_mut() {
        let want = if e.recalled { theme::BTN_DIM } else { theme::BTN_DANGER };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

fn update_site_cards(
    lang: Res<Lang>,
    selected: Res<SelectedSite>,
    mut bgs: Query<(&SiteBtnBg, &mut BackgroundColor)>,
    mut names: Query<(&SiteNameText, &mut Text), Without<SiteInfoText>>,
    mut infos: Query<(&SiteInfoText, &mut Text), Without<SiteNameText>>,
) {
    let lang = *lang;
    for (marker, mut t) in &mut names {
        let new = i18n_expedition::site_name(marker.0, lang);
        if t.0 != new {
            t.0 = new.to_string();
        }
    }
    for (marker, mut t) in &mut infos {
        let site = marker.0;
        let hazard_pct = (site.hazard_per_member_day() * 100.0).round() as u32;
        let mut new = format!(
            "{}\n{}\n{}",
            i18n_expedition::site_desc(site, lang),
            i18n_expedition::site_stats(site.days(), hazard_pct, lang),
            i18n_expedition::site_haul_line(site.haul_per_member_day(), lang),
        );
        if site.rescue_chance() > 0.0 {
            new.push('\n');
            new.push_str(i18n_expedition::rescue_note(lang));
        }
        if t.0 != new {
            t.0 = new;
        }
    }
    for (marker, mut bg) in &mut bgs {
        let want = if marker.0 == selected.0 { theme::BTN_ACTIVE } else { theme::BTN };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// Roster candidates for the party picker, sorted for a stable on-screen
/// order (unlike `roster.rs`'s idle-first sort, there's no "actionable"
/// ordering here — everyone not already away is an equally valid pick).
fn party_candidates(state: &GameState) -> Vec<&Survivor> {
    let mut v: Vec<&Survivor> = state.survivors.iter().collect();
    v.sort_by_key(|s| s.id);
    v
}

enum RowTag {
    Selected,
    Child,
    Sick,
    PartyFull,
    NeededAtHome,
    Available,
}

fn row_tag(state: &GameState, s: &Survivor, selected: &[u32]) -> RowTag {
    if selected.contains(&s.id) {
        return RowTag::Selected;
    }
    if s.stage() == LifeStage::Child {
        return RowTag::Child;
    }
    if s.is_sick() {
        return RowTag::Sick;
    }
    if selected.len() >= EXPEDITION_MAX_PARTY {
        return RowTag::PartyFull;
    }
    if state.survivors.len().saturating_sub(selected.len() + 1) < EXPEDITION_MIN_STAY_HOME {
        return RowTag::NeededAtHome;
    }
    RowTag::Available
}

#[allow(clippy::too_many_arguments)]
fn update_party_rows(
    view: Res<GameView>,
    lang: Res<Lang>,
    mut selected: ResMut<SelectedParty>,
    #[allow(clippy::type_complexity)]
    mut header: Query<&mut Text, (With<PartyHeaderText>, Without<PartyNameText>, Without<PartyTagText>, Without<MoreText>)>,
    mut rows: Query<(&PartyRow, &mut Node)>,
    // No `BaseColor` on this button (see the spawn site): its color is owned
    // entirely by this system, same reasoning as `roster::RosterRowBtn`.
    mut row_btns: Query<(&mut PartyRowBtn, &mut BackgroundColor)>,
    #[allow(clippy::type_complexity)]
    mut names: Query<(&PartyNameText, &mut Text), (Without<PartyHeaderText>, Without<PartyTagText>, Without<MoreText>)>,
    #[allow(clippy::type_complexity)]
    mut tags: Query<(&PartyTagText, &mut Text, &mut TextColor), (Without<PartyHeaderText>, Without<PartyNameText>, Without<MoreText>)>,
    mut more: Query<(&mut Text, &mut Node), (With<MoreText>, Without<PartyRow>, Without<PartyHeaderText>, Without<PartyNameText>, Without<PartyTagText>)>,
) {
    let lang = *lang;
    let Some(state) = view.state.as_ref() else { return };

    // Drop anyone from the pending party who's no longer a legal pick at all
    // (died, or became a child/sick since being selected can't happen, but
    // dying can) -- mirrors `roster::SurvivorSelection`'s cleanup.
    selected.0.retain(|id| state.survivors.iter().any(|s| s.id == *id));

    let candidates = party_candidates(state);

    if let Ok(mut t) = header.single_mut() {
        let new = format!(
            "{} — {}",
            i18n_expedition::party_section(lang),
            i18n_expedition::party_count(selected.0.len(), EXPEDITION_MIN_PARTY, EXPEDITION_MAX_PARTY, lang)
        );
        if t.0 != new {
            t.0 = new;
        }
    }

    for (row, mut node) in &mut rows {
        let shown = row.0 < candidates.len().min(PARTY_ROWS);
        let d = if shown { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    for (mut btn, mut bg) in &mut row_btns {
        btn.survivor = candidates.get(btn.row).map(|s| s.id);
        let want = if btn.survivor.is_some_and(|id| selected.0.contains(&id)) {
            theme::BTN_ACTIVE
        } else {
            theme::BG_SECTION
        };
        if bg.0 != want {
            bg.0 = want;
        }
    }
    for (marker, mut t) in &mut names {
        let new = candidates.get(marker.0).map(|s| s.name.clone()).unwrap_or_default();
        if t.0 != new {
            t.0 = new;
        }
    }
    for (marker, mut t, mut c) in &mut tags {
        let (new, col) = match candidates.get(marker.0) {
            Some(&s) => match row_tag(state, s, &selected.0) {
                RowTag::Selected => (i18n_expedition::selected_tag(lang).to_string(), theme::SUCCESS),
                RowTag::Child => (i18n_expedition::child_tag(lang).to_string(), theme::TEXT_FAINT),
                RowTag::Sick => (super::i18n_panels::sick_tag(lang).to_string(), theme::DANGER),
                RowTag::PartyFull => (i18n_expedition::party_full_tag(lang).to_string(), theme::TEXT_FAINT),
                RowTag::NeededAtHome => (i18n_expedition::needed_at_home_tag(lang).to_string(), theme::TEXT_FAINT),
                RowTag::Available => (roster::profession_level_tag(s, lang), theme::TEXT_MUTED),
            },
            None => (String::new(), theme::TEXT_MUTED),
        };
        if t.0 != new {
            t.0 = new;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
    if let Ok((mut t, mut node)) = more.single_mut() {
        if candidates.len() > PARTY_ROWS {
            let n = candidates.len() - PARTY_ROWS;
            let new = super::i18n_panels::roster_more(n, lang);
            if t.0 != new {
                t.0 = new;
            }
            if node.display != Display::Flex {
                node.display = Display::Flex;
            }
        } else if node.display != Display::None {
            node.display = Display::None;
        }
    }
}

fn update_status_and_launch(
    view: Res<GameView>,
    lang: Res<Lang>,
    selected_site: Res<SelectedSite>,
    selected: Res<SelectedParty>,
    #[allow(clippy::type_complexity)]
    mut status: Query<(&mut Text, &mut TextColor), (With<StatusText>, Without<LaunchLabel>)>,
    mut launch_label: Query<&mut Text, (With<LaunchLabel>, Without<StatusText>)>,
    mut launch_bg: Query<&mut BackgroundColor, With<LaunchBtn>>,
) {
    let lang = *lang;
    if let Ok(mut t) = launch_label.single_mut() {
        if t.0 != i18n_expedition::launch_btn(lang) {
            t.0 = i18n_expedition::launch_btn(lang).to_string();
        }
    }

    let Some(state) = view.state.as_ref() else { return };
    let site = selected_site.0;
    let provisions = site.provisions_per_member_day() * site.days() * selected.0.len() as f32;
    let valid = state.can_launch_expedition(site, &selected.0);
    let afford = state.stock.food >= provisions;
    let ready = valid.is_ok() && afford;

    let (msg, col) = match valid {
        // Sim-authored refusal text, shown as-is (see the module doc).
        Err(reason) => (reason.to_string(), theme::DANGER),
        Ok(()) if !afford => (i18n_expedition::not_enough_food(lang).to_string(), theme::DANGER),
        Ok(()) => (i18n_expedition::provisions_line(provisions, lang), theme::TEXT_MUTED),
    };
    if let Ok((mut t, mut c)) = status.single_mut() {
        if t.0 != msg {
            t.0 = msg;
        }
        if c.0 != col {
            c.0 = col;
        }
    }
    if let Ok(mut bg) = launch_bg.single_mut() {
        let want = if ready { theme::BTN_SUCCESS } else { theme::BTN_DIM };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

fn site_buttons(mut selected: ResMut<SelectedSite>, clicked: Query<(&Interaction, &SiteBtn), Changed<Interaction>>) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            selected.0 = btn.0;
        }
    }
}

fn party_row_buttons(
    view: Res<GameView>,
    mut selected: ResMut<SelectedParty>,
    clicked: Query<(&Interaction, &PartyRowBtn), Changed<Interaction>>,
) {
    let Some(state) = view.state.as_ref() else { return };
    for (interaction, btn) in &clicked {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Some(id) = btn.survivor else { continue };
        if let Some(pos) = selected.0.iter().position(|&x| x == id) {
            selected.0.remove(pos);
            continue;
        }
        // Re-check eligibility at click time rather than trusting the
        // button's last-drawn color -- `row_tag` is the single source of
        // truth both places read.
        let Some(s) = state.survivors.iter().find(|s| s.id == id) else { continue };
        if matches!(row_tag(state, s, &selected.0), RowTag::Available) {
            selected.0.push(id);
        }
    }
}

fn launch_button(
    net: Res<NetConn>,
    view: Res<GameView>,
    site: Res<SelectedSite>,
    mut selected: ResMut<SelectedParty>,
    clicked: Query<&Interaction, (Changed<Interaction>, With<LaunchBtn>)>,
) {
    if !clicked.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };
    if state.can_launch_expedition(site.0, &selected.0).is_err() {
        return;
    }
    let provisions = site.0.provisions_per_member_day() * site.0.days() * selected.0.len() as f32;
    if state.stock.food < provisions {
        return;
    }
    net.send(ClientMsg::Cmd(PlayerCommand::LaunchExpedition { site: site.0, members: selected.0.clone() }));
    selected.0.clear();
}

fn recall_button(net: Res<NetConn>, view: Res<GameView>, clicked: Query<&Interaction, (Changed<Interaction>, With<RecallBtn>)>) {
    if !clicked.iter().any(|i| *i == Interaction::Pressed) {
        return;
    }
    let Some(state) = view.state.as_ref() else { return };
    let Some(e) = state.expeditions.first() else { return };
    if e.recalled {
        return;
    }
    net.send(ClientMsg::Cmd(PlayerCommand::RecallExpedition { expedition: e.id }));
}
