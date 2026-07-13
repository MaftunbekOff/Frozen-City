use bevy::prelude::*;

/// Whether the social modal is open (also gates world/camera input, same
/// idiom as `research::ResearchOpen`).
#[derive(Resource, Default)]
pub struct SocialOpen(pub bool);

#[derive(Component)]
pub(crate) struct SocialRoot;

#[derive(Component)]
pub(crate) struct FriendRow(pub(crate) usize);

#[derive(Component)]
pub(crate) struct FriendName(pub(crate) usize);

#[derive(Component)]
pub(crate) struct VisitBtn {
    pub(crate) row: usize,
    pub(crate) account: Option<i64>,
}

#[derive(Component)]
pub(crate) struct InviteBtn {
    pub(crate) row: usize,
    pub(crate) account: Option<i64>,
}

#[derive(Component)]
pub(crate) struct RemoveBtn {
    pub(crate) row: usize,
    pub(crate) account: Option<i64>,
}

/// Text typed into the "Add friend" field, and whether it has keyboard focus.
/// Kept separate from `menu::LoginForm` — different screen, different shape.
#[derive(Resource, Default)]
pub(crate) struct AddFriendForm {
    pub(crate) text: String,
    pub(crate) focus: bool,
}

#[derive(Component)]
pub(crate) struct AddFriendBox;

#[derive(Component)]
pub(crate) struct AddFriendText;

#[derive(Component)]
pub(crate) struct AddFriendSubmitBtn;

#[derive(Component)]
pub(crate) struct RefreshBtn;

/// Incoming-invite toast (separate from the panel — visible even with the
/// panel closed, like a normal notification).
#[derive(Component)]
pub(crate) struct InviteToastRoot;

#[derive(Component)]
pub(crate) struct InviteToastText;

#[derive(Component)]
pub(crate) struct InviteAcceptBtn;

/// HUD button that toggles the panel — mobile has no `F` key.
#[derive(Component)]
pub(crate) struct SocialHudBtn;

/// "Visiting <name>'s city" indicator + return-home button, shown only while
/// `Session.visiting.is_some()`. Distinct from `ui::WorldSwitchBtn` (which
/// only flips between the personal world and the central world) since
/// visiting is a third state layered on top of either.
#[derive(Component)]
pub(crate) struct VisitingRoot;

#[derive(Component)]
pub(crate) struct VisitingText;

#[derive(Component)]
pub(crate) struct GoHomeBtn;

/// "Offline guests" policy row (V0.6 owner-offline entry): shown only for
/// account sessions (`SocialState::visit_policy` is `Some` once the server
/// reported it), toggles `ClientMsg::SetVisitPolicy`.
#[derive(Component)]
pub(crate) struct PolicyRow;

#[derive(Component)]
pub(crate) struct PolicyBtn;

#[derive(Component)]
pub(crate) struct PolicyText;

/// One transient toast line spawned from a `BubbleEvent` (chat bubble or
/// player_id==0 system feedback), independent of the drained inbox itself.
#[derive(Component)]
pub(crate) struct Toast {
    pub(crate) age: f32,
}

#[derive(Component)]
pub(crate) struct ToastRoot;

#[derive(Component)]
pub(crate) struct VisitLabel;

#[derive(Component)]
pub(crate) struct InviteLabel;

/// Every one of these labels is `&mut Text` on its own dedicated marker
/// entity — but Bevy's query-conflict check only recognizes two
/// `Query<&mut Text, With<A>>` / `Query<&mut Text, With<B>>` parameters as
/// disjoint when at least one side *also* excludes the other's marker
/// (`With<A>` alone vs `With<B>` alone is NOT sufficient — see
/// `bevy_ecs::query::access::FilteredAccess::is_ruled_out_by`, which only
/// rules two queries compatible via an explicit `with`/`without` overlap).
/// Rather than writing eleven mutually-excluding marker structs, all eleven
/// live on one `StaticLabel` enum component instead, read through a single
/// `Query<(&StaticLabel, &mut Text)>` — one `&mut Text` access, no conflict
/// possible.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StaticLabel {
    Title,
    SectionInvite,
    SectionFriends,
    SectionShowcase,
    SectionPolicy,
    AddFriendSubmit,
    Refresh,
    Hud,
    GoHome,
    InviteAccept,
    Policy,
}
