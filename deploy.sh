#!/usr/bin/env bash
# Auto-deploy: pull GitHub main if it moved, test, build, and swap the
# running server — only touching the live service after a clean build.
#
# Safe by construction: the service is stopped only after `cargo test`,
# `cargo build --release` and `build-web.sh` all succeed. A failure at any
# earlier stage leaves the live game untouched and exits non-zero (visible
# in `systemctl status frozen-city-deploy` / the log below).
#
# Run manually:      ./deploy.sh
# Run unattended:     via the frozen-city-deploy.timer (see deploy/ dir)
set -euo pipefail

# systemd services don't source ~/.bashrc, so cargo/rustc are absent from
# PATH there even though they're on it in an interactive shell.
export PATH="$HOME/.cargo/bin:$PATH"

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR=/opt/frozen-city
SERVICE=frozen-city
LOG=/var/log/frozen-city-deploy.log
LOCK=/tmp/frozen-city-deploy.lock

exec 9>"$LOCK"
if ! flock -n 9; then
    echo "$(date -Is) [deploy] already running elsewhere, skipping" | sudo tee -a "$LOG" >/dev/null
    exit 0
fi

log() { echo "$(date -Is) [deploy] $*" | sudo tee -a "$LOG" >/dev/null; }

# Best-effort Telegram notification, reusing the server-monitor bot's
# credentials (same pattern as edustatus-notify.sh: source the env file at
# run time, never hold the token in this script). Silently a no-op if the
# file or its variables are missing.
notify() {
    local text="$1"
    if sudo test -r /etc/server-monitor/env; then
        # shellcheck disable=SC1091
        source <(sudo cat /etc/server-monitor/env)
        if [ -n "${TG_TOKEN:-}" ] && [ -n "${TG_CHAT:-}" ]; then
            curl -s -X POST "https://api.telegram.org/bot${TG_TOKEN}/sendMessage" \
                --data-urlencode "chat_id=${TG_CHAT}" \
                --data-urlencode "text=${text}" \
                --data-urlencode "parse_mode=HTML" >/dev/null || true
        fi
    fi
}

cd "$REPO_DIR"

if ! git fetch origin main --quiet 2>>"$LOG"; then
    log "ERROR: git fetch failed (network?) — see $LOG"
    notify "🧊❌ <b>Frozen City</b>: 'git fetch' failed — check the server's network / GitHub reachability."
    exit 1
fi
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)

if [ "$LOCAL" = "$REMOTE" ]; then
    exit 0  # nothing new; stay quiet so the log doesn't fill with no-ops
fi

log "new commit(s) detected: ${LOCAL:0:7} -> ${REMOTE:0:7}"

if ! git merge-base --is-ancestor "$LOCAL" origin/main; then
    log "ERROR: origin/main is not a fast-forward of local HEAD (history diverged) — refusing to auto-merge. Manual 'git pull' review needed."
    notify "🧊❌ <b>Frozen City</b>: deploy blocked — origin/main diverged from the local branch (not a fast-forward). Needs a manual 'git pull' / review."
    exit 1
fi

if ! git pull --ff-only origin main >>/dev/null 2>&1; then
    log "ERROR: git pull --ff-only failed unexpectedly"
    notify "🧊❌ <b>Frozen City</b>: 'git pull --ff-only' failed unexpectedly — see $LOG"
    exit 1
fi
log "pulled $(git log -1 --format='%h %s')"

COMMIT_MSG=$(git log -1 --format='%s' "$REMOTE")
HEADER="${LOCAL:0:7} → ${REMOTE:0:7}
${COMMIT_MSG}"

notify "🧊 <b>Frozen City</b>: new commit, testing…
${HEADER}"

log "running tests..."
# --workspace: without it, `cargo test` from this root package only runs
# frozen_city's own tests/*.rs — fc-net's and fc-game's own internal
# `#[cfg(test)]` unit tests (e.g. persist.rs's save-format migration tests,
# accounts.rs's auth tests) silently never ran under this gate at all.
if ! cargo test --release --workspace >>"$LOG" 2>&1; then
    log "ERROR: tests failed at ${REMOTE:0:7} — deploy aborted, live service untouched. See $LOG for the failing test."
    notify "🧊❌ <b>Frozen City</b>: deploy failed (tests)
${HEADER}
Live server untouched."
    exit 1
fi

notify "🧊 tests passed ✓ — building native release…"

log "building native release..."
if ! cargo build --release >>"$LOG" 2>&1; then
    log "ERROR: native build failed — deploy aborted, live service untouched."
    notify "🧊❌ <b>Frozen City</b>: deploy failed (native build)
${HEADER}
Live server untouched."
    exit 1
fi

notify "🧊 native build done ✓ — smoke-testing the client…"

# `cargo test` only exercises `src/game`/`src/net` pure logic — it never boots
# the actual Bevy client, so a runtime-only failure (e.g. an ECS system query
# conflict, which Bevy only detects once the schedule actually *runs*) can
# pass every test and still crash the instant a player opens the page. Catch
# that class of bug here, before the slow web build, by actually running the
# game for a few seconds under a virtual display and failing loudly if it
# panics or exits non-zero.
log "smoke-testing the client (native, headless via Xvfb)..."
SMOKE_LOG=$(mktemp)
if ! xvfb-run -a timeout 300 target/release/frozen_city --smoke >"$SMOKE_LOG" 2>&1 || grep -qi "panicked" "$SMOKE_LOG"; then
    log "ERROR: client smoke test failed/panicked — deploy aborted, live service untouched. Output:"
    cat "$SMOKE_LOG" | sudo tee -a "$LOG" >/dev/null
    notify "🧊❌ <b>Frozen City</b>: deploy failed (client smoke test — it boots but crashes)
${HEADER}
Live server untouched."
    rm -f "$SMOKE_LOG"
    exit 1
fi
rm -f "$SMOKE_LOG"

notify "🧊 smoke test passed ✓ — building the web package now (longest step, ~5-9 min)…"

log "building web package..."
if ! ./build-web.sh >>"$LOG" 2>&1; then
    log "ERROR: web build failed — deploy aborted, live service untouched."
    notify "🧊❌ <b>Frozen City</b>: deploy failed (web build)
${HEADER}
Live server untouched."
    exit 1
fi

notify "🧊 web build done ✓ — swapping the live server…"

log "deploying to $DEPLOY_DIR and restarting $SERVICE..."
sudo systemctl stop "$SERVICE"
sudo cp target/release/frozen_city "$DEPLOY_DIR/"
sudo cp -r web/. "$DEPLOY_DIR/web/"
sudo chown -R root:root "$DEPLOY_DIR"
sudo chmod -R a+rX "$DEPLOY_DIR"
sudo systemctl start "$SERVICE"

sleep 2
if systemctl is-active --quiet "$SERVICE"; then
    log "deploy OK — live at ${REMOTE:0:7}"
    notify "🧊✅ <b>Frozen City</b>: deployed
${HEADER}
https://twelfth.uz/game/"
else
    log "ERROR: $SERVICE failed to start after deploy — check 'journalctl -u $SERVICE'"
    notify "🧊🔥 <b>Frozen City</b>: service FAILED to start after deploy at ${REMOTE:0:7}!
Check: journalctl -u $SERVICE"
    exit 1
fi
