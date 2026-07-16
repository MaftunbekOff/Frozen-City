//! Menu overlays. The Account (sign-in / register) and Settings (language /
//! graphics / sound) blocks live in modal panels reached from the landing —
//! keeping the main menu to clean primary actions the way a polished game menu
//! does, instead of dumping a login form and every setting onto the landing.
//!
//! Built on the design system's `theme::scrim` + `theme::modal_panel`, and on
//! the very same marker components the account/settings systems already query
//! (`LoginFieldBox`, `LangButton`, `AudioToggleButton`, …), so those systems
//! keep working unchanged wherever the widgets are spawned. Every rebuild path
//! (open, close, language change) re-runs `spawn_overlay`, so an open modal
//! always re-renders in the current language.

use bevy::prelude::*;

use super::super::i18n::Lang;
use super::super::i18n_menu as mtxt;
use super::super::theme::{self, BaseColor, FormFactor};
use super::super::{AudioSettings, GameView, QualityPref, Screen, Settings};
use super::*;

/// Which overlay modal is open over the menu landing (see `layout::build_menu`).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuOverlay {
    #[default]
    None,
    Account,
    Settings,
}

/// The scrim + modal despawn root, a sibling of `MenuRoot`. Rebuilt whenever
/// the open overlay (or the language) changes — see `overlay_buttons`'s
/// manual despawn+respawn for that same-state case, same reasoning as
/// `MenuRoot`'s doc comment. Unlike `MenuRoot`, this ALSO carries
/// `DespawnOnExit(Screen::Menu)` (see `spawn_overlay`): a real production
/// bug had a successful Login/Register leave this modal orphaned on top of
/// the game — `account_login_button`'s and `handle_control_action`'s Submit
/// paths call `next.set(Screen::Game)` but never explicitly close the
/// overlay, and without this marker nothing else ever did either.
#[derive(Component)]
pub(crate) struct OverlayRoot;

/// A landing button that opens the overlay it carries.
#[derive(Component, Clone, Copy)]
pub(crate) struct OpenOverlayBtn(pub(crate) MenuOverlay);

/// The modal's × button — closes the overlay, back to the landing.
#[derive(Component)]
pub(crate) struct CloseOverlayBtn;

/// Spawns the scrim + modal for `overlay` above the landing; a no-op for
/// `None`. A plain function (not a system) so `build_menu` can call it as the
/// final step of every rebuild.
pub(crate) fn spawn_overlay(
    commands: &mut Commands,
    overlay: MenuOverlay,
    lang: Lang,
    quality_pref: QualityPref,
    audio: AudioSettings,
    ff: FormFactor,
) {
    let title = match overlay {
        MenuOverlay::None => return,
        MenuOverlay::Account => mtxt::section_account(lang),
        MenuOverlay::Settings => mtxt::section_settings(lang),
    };
    commands
        .spawn((theme::scrim(ff), OverlayRoot, GlobalZIndex(10), DespawnOnExit(Screen::Menu)))
        .with_children(|scrim| {
            // The panel itself captures clicks (its own `Interaction`) so a
            // click inside the modal never falls through to anything behind.
            scrim
                .spawn((theme::modal_panel(ff), Interaction::default()))
                .with_children(|panel| {
                    // Header: title on the left, a × close button on the right.
                    panel
                        .spawn(Node {
                            width: Val::Percent(100.0),
                            flex_direction: FlexDirection::Row,
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        })
                        .with_children(|head| {
                            head.spawn(theme::section(title));
                            head.spawn((
                                Button,
                                Node {
                                    width: Val::Px(ff.btn_h()),
                                    height: Val::Px(ff.btn_h()),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(theme::BTN),
                                BaseColor(theme::BTN),
                                CloseOverlayBtn,
                            ))
                            .with_children(|b| {
                                b.spawn(theme::text("\u{00D7}", 20.0, theme::TEXT_PRIMARY));
                            });
                        });
                    panel.spawn(theme::divider());

                    match overlay {
                        MenuOverlay::Account => account_modal(panel, lang, ff),
                        MenuOverlay::Settings => {
                            settings_modal(panel, lang, quality_pref, audio, ff)
                        }
                        MenuOverlay::None => {}
                    }
                });
        });
}

/// The Account modal body: intro, the three sign-in fields, the Sign-in /
/// Register row, and a local error line. Same widgets (and markers) the old
/// inline account section used, so `login_field_focus`, `login_form_keyboard`,
/// `account_login_button`, `register_toggle_button`, `update_login_fields` and
/// `update_register_toggle` all drive it unchanged.
fn account_modal(panel: &mut ChildSpawnerCommands, lang: Lang, ff: FormFactor) {
    panel.spawn((
        theme::text(mtxt::account_intro(lang), theme::FS_SMALL, theme::TEXT_MUTED),
        Node { width: Val::Percent(100.0), ..default() },
    ));

    panel
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(theme::SP_SM),
            ..default()
        })
        .with_children(|fields| {
            for field in [AccountField::Name, AccountField::Login, AccountField::Password] {
                fields
                    .spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            height: Val::Px(ff.btn_h()),
                            justify_content: JustifyContent::FlexStart,
                            align_items: AlignItems::Center,
                            padding: UiRect::horizontal(Val::Px(theme::SP_MD)),
                            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                            ..default()
                        },
                        BackgroundColor(FIELD_BG),
                        LoginFieldBox(field),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            theme::text("", theme::FS_BODY, theme::TEXT_MUTED),
                            LoginFieldText(field),
                        ));
                    });
            }
        });

    panel
        .spawn(menu_row())
        .with_children(|row| {
            row.spawn((
                Button,
                Node {
                    width: Val::Auto,
                    min_height: Val::Px(ff.btn_h()),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_XS)),
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(theme::BTN_ACTIVE),
                BaseColor(theme::BTN_ACTIVE),
            ))
            .insert(AccountLoginButton)
            .with_children(|b| {
                b.spawn((
                    theme::text(mtxt::btn_sign_in(lang), theme::FS_BODY, theme::TEXT_PRIMARY),
                    AccountLoginButtonLabel,
                ));
            });
            row.spawn((
                Button,
                Node {
                    width: Val::Auto,
                    min_height: Val::Px(ff.btn_h()),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_XS)),
                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                    ..default()
                },
                BackgroundColor(FIELD_BG),
                BaseColor(FIELD_BG),
                RegisterToggleButton,
            ))
            .with_children(|b| {
                b.spawn((
                    theme::text(mtxt::btn_sign_up(lang), theme::FS_SMALL, theme::TEXT_MUTED),
                    RegisterToggleLabel,
                ));
            });
        });

    // A MenuErrorText inside the modal so a failed sign-in/register shows here
    // (the landing's own copy is behind the scrim). Every error writer targets
    // ALL `MenuErrorText`, so both stay in sync.
    panel.spawn((
        theme::text("", theme::FS_SMALL, theme::DANGER),
        Node { width: Val::Percent(100.0), ..default() },
        MenuErrorText,
    ));
}

/// The Settings modal body: language, graphics quality and sound — the same
/// widgets (and markers) the old inline settings section used, so
/// `lang_buttons`, `quality_buttons`, `audio_toggle_button` and
/// `update_settings_buttons` all drive it unchanged.
fn settings_modal(
    panel: &mut ChildSpawnerCommands,
    lang: Lang,
    quality_pref: QualityPref,
    audio: AudioSettings,
    ff: FormFactor,
) {
    panel.spawn(theme::text(mtxt::language_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
    panel
        .spawn(menu_row())
        .with_children(|row| {
            for l in [Lang::Uz, Lang::En, Lang::Ru] {
                let bg = if lang == l { theme::BTN_ACTIVE } else { theme::BTN };
                row.spawn(theme::button(Val::Percent(100.0 / 3.0), ff.btn_h(), bg))
                    .insert(LangButton(l))
                    .with_children(|b| {
                        b.spawn(theme::text(l.label(), theme::FS_SMALL, theme::TEXT_PRIMARY));
                    });
            }
        });

    panel.spawn(theme::text(mtxt::graphics_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
    panel
        .spawn(menu_row())
        .with_children(|row| {
            for (q, label) in [
                (QualityPref::Auto, mtxt::quality_auto(lang)),
                (QualityPref::Low, mtxt::quality_low(lang)),
                (QualityPref::Medium, mtxt::quality_medium(lang)),
                (QualityPref::High, mtxt::quality_high(lang)),
            ] {
                let bg = if quality_pref == q { theme::BTN_ACTIVE } else { theme::BTN };
                row.spawn(theme::button(Val::Percent(25.0), ff.btn_h(), bg))
                    .insert(QualityButton(q))
                    .with_children(|b| {
                        b.spawn(theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY));
                    });
            }
        });

    panel.spawn(theme::text(mtxt::sound_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
    panel
        .spawn(theme::button(
            Val::Percent(100.0),
            ff.btn_h(),
            if audio.enabled { theme::BTN_ACTIVE } else { theme::BTN },
        ))
        .insert(AudioToggleButton)
        .with_children(|b| {
            let label = if audio.enabled { mtxt::sound_on(lang) } else { mtxt::sound_off(lang) };
            b.spawn((theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY), AudioToggleLabel));
        });
}

/// Opens/closes the overlay: the landing's Account/Settings buttons open one,
/// the modal's × closes it. Any change rebuilds the whole menu (landing +
/// modal) through [`build_menu`], the same despawn-and-respawn idiom
/// `lang_buttons` uses, so the new state is reflected everywhere at once.
#[allow(clippy::too_many_arguments)]
pub fn overlay_buttons(
    mut commands: Commands,
    open_q: Query<(&Interaction, &OpenOverlayBtn), Changed<Interaction>>,
    close_q: Query<&Interaction, (With<CloseOverlayBtn>, Changed<Interaction>)>,
    roots: Query<Entity, Or<(With<MenuRoot>, With<OverlayRoot>)>>,
    mut overlay: ResMut<MenuOverlay>,
    mut form: ResMut<LoginForm>,
    settings: Res<Settings>,
    view: Res<GameView>,
    lang: Res<Lang>,
    quality_pref: Res<QualityPref>,
    audio: Res<AudioSettings>,
    ff: Res<FormFactor>,
) {
    let mut target: Option<MenuOverlay> = None;
    for (interaction, btn) in &open_q {
        if *interaction == Interaction::Pressed {
            target = Some(btn.0);
        }
    }
    for interaction in &close_q {
        if *interaction == Interaction::Pressed {
            target = Some(MenuOverlay::None);
        }
    }
    let Some(next) = target else { return };
    if next == *overlay {
        return;
    }
    *overlay = next;
    // The Account modal's fields (and their entities) are about to be
    // despawned below — clear any lingering focus so a later reopen doesn't
    // inherit a stale focus, and (wasm) so the mobile keyboard doesn't stay
    // visually up after the modal it was typing into is gone.
    form.focus = None;
    #[cfg(target_arch = "wasm32")]
    blur_proxy();
    for e in &roots {
        commands.entity(e).despawn();
    }
    let error = view.error.clone().unwrap_or_default();
    build_menu(commands, &settings, error, *lang, *quality_pref, *audio, *ff, *overlay);
}
