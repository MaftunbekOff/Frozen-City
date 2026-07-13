use bevy::prelude::*;

use frozen_city::game::types::{BuildingKind, PlayerCommand};
use frozen_city::net::protocol::ClientMsg;

use super::super::i18n::Lang;
use super::super::i18n_hud;
use super::super::i18n_names;
use super::super::theme::{self, BTN, BTN_ACTIVE, BTN_DIM, BTN_HOVER};
use super::super::*;
use super::*;

pub fn build_buttons(
    view: Res<GameView>,
    lang: Res<Lang>,
    ff: Res<theme::FormFactor>,
    mut build: ResMut<BuildMode>,
    clicked: Query<(&Interaction, &BuildBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &BuildBtn, &mut BackgroundColor)>,
    mut tooltip: Query<&mut Text, With<TooltipText>>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            build.0 = if build.0 == Some(btn.0) {
                None
            } else {
                Some(btn.0)
            };
        }
    }

    let wood = view
        .state
        .as_ref()
        .map(|s| s.stock.wood)
        .unwrap_or_default();
    let mut hovered: Option<BuildingKind> = None;
    for (interaction, btn, mut bg) in &mut all {
        let affordable = wood >= btn.0.cost_wood() as f32;
        if *interaction == Interaction::Hovered {
            hovered = Some(btn.0);
        }
        let color = if build.0 == Some(btn.0) {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else if !affordable {
            BTN_DIM
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }

    if let Ok(mut tip) = tooltip.single_mut() {
        let lang = *lang;
        let new = match hovered.or(build.0) {
            Some(k) => i18n_hud::build_tooltip(
                i18n_names::building_name(k, lang),
                k.cost_wood(),
                i18n_names::building_desc(k, lang),
                lang,
            ),
            None if ff.compact() => i18n_hud::default_hint_mobile(lang).to_string(),
            None => i18n_hud::default_hint_desktop(lang).to_string(),
        };
        if tip.0 != new {
            tip.0 = new;
        }
    }
}

pub fn furnace_buttons(
    view: Res<GameView>,
    net: Res<NetConn>,
    clicked: Query<(&Interaction, &FurnaceLvlBtn), Changed<Interaction>>,
    mut all: Query<(&Interaction, &FurnaceLvlBtn, &mut BackgroundColor)>,
) {
    for (interaction, btn) in &clicked {
        if *interaction == Interaction::Pressed {
            net.send(ClientMsg::Cmd(PlayerCommand::SetFurnaceLevel { level: btn.0 }));
        }
    }
    let current = view
        .state
        .as_ref()
        .map(|s| s.furnace_level)
        .unwrap_or(1);
    for (interaction, btn, mut bg) in &mut all {
        let color = if btn.0 == current {
            BTN_ACTIVE
        } else if *interaction == Interaction::Hovered {
            BTN_HOVER
        } else {
            BTN
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}
