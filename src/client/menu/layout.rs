//! The menu's visual structure: the section/row layout helpers and the full
//! spawn (`spawn_menu`, `build_menu`).

use bevy::prelude::*;

use super::super::i18n::Lang;
use super::super::i18n_menu as mtxt;
use super::super::theme::{self, BaseColor, FormFactor};
use super::super::{AudioSettings, GameView, QualityPref, Screen, Settings};
use super::*;

/// Vertical section card for the menu (Play/Account/Region/Settings): unlike
/// `theme::card()` (a horizontal list-row idiom used by `research.rs`), each
/// menu section stacks a header row on top of its content, and its content
/// must stay within the card's own width — a plain `theme::card()` here would
/// lay the section label and its content out side-by-side, pushing content
/// off the right edge of the card (and off-screen on Mobile).
fn menu_section() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            width: Val::Percent(100.0),
            padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_SM)),
            row_gap: Val::Px(theme::SP_SM),
            border: UiRect::all(Val::Px(1.0)),
            border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
            ..default()
        },
        BackgroundColor(theme::BG_SECTION),
        BorderColor::all(theme::BORDER),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.35),
            Val::Px(0.0),
            Val::Px(6.0),
            Val::Px(0.0),
            Val::Px(16.0),
        ),
    )
}

/// A wrapping content row inside a `menu_section` (e.g. the Language or
/// Graphics button group): full card width, and `FlexWrap::Wrap` so a long
/// label/localization or a narrow (Mobile) card reflows onto a second line
/// instead of spilling past the card's right edge.
fn menu_row() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_wrap: FlexWrap::Wrap,
        column_gap: Val::Px(theme::SP_SM),
        row_gap: Val::Px(theme::SP_SM),
        ..default()
    }
}

/// Marks the single despawn/respawn root of the whole menu screen, so a
/// settings change that needs the menu rebuilt (currently: language) can find
/// and despawn it without relying on `DespawnOnExit(Screen::Menu)` (which only
/// fires on an actual state exit, not a same-state respawn).
#[derive(Component)]
pub(crate) struct MenuRoot;

pub fn spawn_menu(
    commands: Commands,
    settings: Res<Settings>,
    view: Res<GameView>,
    lang: Res<Lang>,
    quality_pref: Res<QualityPref>,
    audio: Res<AudioSettings>,
    ff: Res<FormFactor>,
) {
    let error = view.error.clone().unwrap_or_default();
    build_menu(commands, &settings, error, *lang, *quality_pref, *audio, *ff);
}

/// The actual menu layout, factored out of the `spawn_menu` system so
/// `lang_buttons` can rebuild it too (from a click handler, it already holds
/// plain values/`&mut`s, not fresh `Res<T>` system params — `Res`/`ResMut`
/// can only be obtained by the scheduler injecting them into a system's
/// signature, not constructed by hand). Both callers pass an owned/copied
/// snapshot of each resource, never the resource references themselves.
///
/// Layout: a single scrolling column (full-width on Mobile, a centered
/// ~480px card on Tablet/Desktop) — big title, subtitle, error line, then
/// four `theme::card` sections (Play / Account / Region / Settings) and a
/// dim hint footer. Region only appears on wasm (see `RegionButton`'s doc).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_menu(
    mut commands: Commands,
    settings: &Settings,
    error: String,
    lang: Lang,
    quality_pref: QualityPref,
    audio: AudioSettings,
    ff: FormFactor,
) {
    let column_width = if ff.compact() { Val::Percent(100.0) } else { Val::Px(480.0) };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                right: Val::Px(0.0),
                top: Val::Px(0.0),
                bottom: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Center,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            // Fon menu_fx gradient qatlamidan keladi (GlobalZIndex -2).
            BackgroundColor(Color::NONE),
            ScrollPosition::default(),
            DespawnOnExit(Screen::Menu),
            MenuRoot,
        ))
        .with_children(|p| {
            p.spawn((
                Node {
                    width: column_width,
                    max_width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Stretch,
                    padding: UiRect::all(Val::Px(theme::SP_MD)).with_top(Val::Px(theme::SP_XL)),
                    row_gap: Val::Px(theme::SP_LG),
                    ..default()
                },
            ))
            .with_children(|col| {
                col.spawn(Node {
                    align_items: AlignItems::Center,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(theme::SP_XS),
                    ..default()
                })
                .with_children(|head| {
                    head.spawn((
                        theme::text(
                            mtxt::title(lang),
                            if ff.compact() { 44.0 } else { 54.0 },
                            theme::TEXT_PRIMARY,
                        ),
                        // Muzli "porlash": pastga siljigan ko'k soya.
                        TextShadow {
                            offset: Vec2::new(0.0, 4.0),
                            color: Color::srgba(0.15, 0.55, 0.90, 0.35),
                        },
                    ));
                    head.spawn((
                        Node {
                            width: Val::Px(64.0),
                            height: Val::Px(3.0),
                            margin: UiRect::vertical(Val::Px(theme::SP_XS)),
                            border_radius: BorderRadius::MAX,
                            ..default()
                        },
                        BackgroundColor(theme::ACCENT_ICE),
                    ));
                    head.spawn(theme::text(mtxt::subtitle(lang), theme::FS_BODY, theme::ACCENT_ICE));
                    head.spawn((theme::text(error, theme::FS_BODY, theme::DANGER), MenuErrorText));
                });

                // ---------------------------------------------------- Play
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_play(lang)));
                    section.spawn(theme::divider());

                    // The browser cannot listen for connections or quit the
                    // page, so it only offers Singleplayer and Join.
                    let mut buttons: Vec<(MenuAction, String)> =
                        vec![(MenuAction::Single, mtxt::btn_singleplayer(lang).to_string())];
                    #[cfg(not(target_arch = "wasm32"))]
                    buttons.push((MenuAction::Host, mtxt::btn_host_coop(lang, settings.host_port)));
                    buttons.push((MenuAction::Join, mtxt::btn_join_guest(lang, &settings.join_addr)));
                    #[cfg(not(target_arch = "wasm32"))]
                    buttons.push((MenuAction::Quit, mtxt::btn_quit(lang).to_string()));
                    for (action, label) in buttons {
                        // Yakka o'yin — asosiy CTA: kattaroq, issiq amber
                        // ("pechni yoq") — qolganlari ikkilamchi.
                        let primary = action == MenuAction::Single;
                        let bg = if primary { theme::BTN_ACTIVE } else { theme::BTN };
                        section
                            .spawn((
                                Button,
                                Node {
                                    width: Val::Percent(100.0),
                                    height: Val::Auto,
                                    min_height: Val::Px(if primary {
                                        ff.btn_h() + 12.0
                                    } else {
                                        ff.btn_h()
                                    }),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    padding: UiRect::axes(Val::Px(theme::SP_MD), Val::Px(theme::SP_SM)),
                                    border_radius: BorderRadius::all(Val::Px(theme::RAD_BTN)),
                                    ..default()
                                },
                                BackgroundColor(bg),
                                BaseColor(bg),
                            ))
                            .insert(action)
                            .with_children(|b| {
                                b.spawn(theme::text(
                                    label,
                                    if primary { 17.0 } else { theme::FS_BODY },
                                    theme::TEXT_PRIMARY,
                                ));
                            });
                    }
                });

                // ---------------------------------------------------Account
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_account(lang)));
                    section.spawn(theme::divider());
                    // Its own full-width row so long/localized intro text
                    // wraps at the card's width instead of squeezed into a
                    // narrow auto-width column alongside the fields below.
                    section.spawn((
                        theme::text(mtxt::account_intro(lang), theme::FS_SMALL, theme::TEXT_MUTED),
                        Node {
                            width: Val::Percent(100.0),
                            ..default()
                        },
                    ));

                    section
                        .spawn(Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: Val::Px(theme::SP_SM),
                            ..default()
                        })
                        .with_children(|fields| {
                            // Name is register-only; its box is hidden
                            // (Display::None) in login mode by
                            // `update_login_fields`.
                            for field in
                                [AccountField::Name, AccountField::Login, AccountField::Password]
                            {
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

                    section
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
                                BackgroundColor(theme::BTN),
                                BaseColor(theme::BTN),
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
                });

                // ---------------------------------------------------Region
                // Which reverse-proxy path (and thus which independent
                // region-server process) Join/sign-in dial. Native desktop
                // only ever dials a LAN address directly, so this section is
                // browser-only.
                #[cfg(target_arch = "wasm32")]
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_region(lang)));
                    section.spawn(theme::divider());
                    section.spawn(theme::text(mtxt::region_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for (n, path) in [(1u8, "/ws"), (2, "/ws-r2"), (3, "/ws-r3")] {
                                row.spawn(theme::button(Val::Percent(100.0 / 3.0), ff.btn_h(), theme::BTN))
                                    .insert(RegionButton(path))
                                    .with_children(|b| {
                                        b.spawn(theme::text(
                                            mtxt::region_name(n, lang),
                                            theme::FS_SMALL,
                                            theme::TEXT_PRIMARY,
                                        ));
                                    });
                            }
                        });
                });

                // -------------------------------------------------Settings
                col.spawn(menu_section()).with_children(|section| {
                    section.spawn(theme::section(mtxt::section_settings(lang)));
                    section.spawn(theme::divider());

                    section.spawn(theme::text(mtxt::language_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for l in [Lang::Uz, Lang::En, Lang::Ru] {
                                let bg = if lang == l { BTN_ACTIVE } else { theme::BTN };
                                row.spawn(theme::button(Val::Percent(100.0 / 3.0), ff.btn_h(), bg))
                                    .insert(LangButton(l))
                                    .with_children(|b| {
                                        b.spawn(theme::text(l.label(), theme::FS_SMALL, theme::TEXT_PRIMARY));
                                    });
                            }
                        });

                    section.spawn(theme::text(mtxt::graphics_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(menu_row())
                        .with_children(|row| {
                            for (q, label) in [
                                (QualityPref::Auto, mtxt::quality_auto(lang)),
                                (QualityPref::Low, mtxt::quality_low(lang)),
                                (QualityPref::Medium, mtxt::quality_medium(lang)),
                                (QualityPref::High, mtxt::quality_high(lang)),
                            ] {
                                let bg = if quality_pref == q { BTN_ACTIVE } else { theme::BTN };
                                row.spawn(theme::button(Val::Percent(25.0), ff.btn_h(), bg))
                                    .insert(QualityButton(q))
                                    .with_children(|b| {
                                        b.spawn(theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY));
                                    });
                            }
                        });

                    section.spawn(theme::text(mtxt::sound_label(lang), theme::FS_SMALL, theme::TEXT_MUTED));
                    section
                        .spawn(theme::button(
                            Val::Percent(100.0),
                            ff.btn_h(),
                            if audio.enabled { BTN_ACTIVE } else { theme::BTN },
                        ))
                        .insert(AudioToggleButton)
                        .with_children(|b| {
                            let label = if audio.enabled { mtxt::sound_on(lang) } else { mtxt::sound_off(lang) };
                            b.spawn((theme::text(label, theme::FS_SMALL, theme::TEXT_PRIMARY), AudioToggleLabel));
                        });
                });

                col.spawn(Node {
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(theme::SP_XS),
                    ..default()
                })
                .with_children(|f| {
                    f.spawn(theme::text(
                        mtxt::hint_playing_as(lang, &settings.name, settings.win_days),
                        theme::FS_MICRO,
                        theme::TEXT_FAINT,
                    ));
                    f.spawn(theme::text(mtxt::hint_controls(lang), theme::FS_MICRO, theme::TEXT_FAINT));
                });
            });
        });
}
