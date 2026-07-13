use fc_game::types::{GameState, LedgerEntry};

use crate::accounts;
use crate::persist;
use crate::protocol::{FriendInfo, ServerMsg, ShowcaseEntry};

/// The freshest friends list for `account`, with online-in-central flags when
/// this world IS the central one (elsewhere the server has no global view and
/// reports everyone offline — see `FriendInfo::online_central`).
pub(crate) fn social_for(state: &GameState, account: i64) -> ServerMsg {
    let friends = accounts::friends_list(account)
        .into_iter()
        .map(|(fid, fname)| FriendInfo {
            account: fid,
            name: fname,
            online_central: state.central && state.players.iter().any(|p| p.account == Some(fid)),
        })
        .collect();
    ServerMsg::Social { friends }
}

/// `ServerMsg::Showcase` for `account` (V0.5 "hub activities v1"): one entry
/// per friend, read on demand from that friend's personal-world save file —
/// disk I/O, so the tick loop never calls this directly; it snapshots the
/// two bits of sim state needed here (`central`, the ledger) and runs this
/// on a throwaway thread (see the `RefreshShowcase` handler). A friend with
/// no save yet (never played, or the file simply isn't there) is silently
/// skipped rather than fabricating a row. When the requesting world IS the
/// central one, each entry's `central_contribution` is filled in from its
/// ledger; elsewhere (a personal world has no global view) it's `None`.
pub(crate) fn showcase_for(central: bool, ledger: &[LedgerEntry], account: i64) -> ServerMsg {
    let entries = accounts::friends_list(account)
        .into_iter()
        .filter_map(|(fid, fname)| {
            let path = crate::world_manager::account_save_path(fid);
            let friend_state = persist::load_at(&path)?;
            Some(ShowcaseEntry {
                account: fid,
                name: fname,
                days_survived: friend_state.day(),
                population: friend_state.survivors.len() as u32,
                buildings: friend_state.buildings.len() as u32,
                graduated: friend_state.graduated,
                central_contribution: central.then(|| {
                    ledger
                        .iter()
                        .find(|e| e.account == fid)
                        .map(|e| e.totals)
                        .unwrap_or_default()
                }),
            })
        })
        .collect();
    ServerMsg::Showcase { entries }
}

/// A private, transient system line to one client, reusing the Bubble channel
/// (`player_id: 0` marks system text, same convention as `ChatLine`). Used
/// for feedback that must not enter the shared world snapshot ("friend not
/// found", "invite sent").
pub(crate) fn system_bubble(text: &str) -> ServerMsg {
    ServerMsg::Bubble {
        player_id: 0,
        name: "System".to_string(),
        color: 0,
        text: text.to_string(),
    }
}

pub(crate) fn sanitize_name(name: &str) -> String {
    let cleaned: String = name.chars().filter(|c| !c.is_control()).take(24).collect();
    if cleaned.trim().is_empty() {
        "Survivor".to_string()
    } else {
        cleaned.trim().to_string()
    }
}
