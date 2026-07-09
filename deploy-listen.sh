#!/usr/bin/env bash
# Waits for a "gitup" message in the server-monitor bot's Telegram chat and
# runs deploy.sh once when it arrives. No polling of GitHub — deploys happen
# only when explicitly requested, replacing the old 5-minute timer.
#
# Credentials are read at run time from /etc/server-monitor/env (same file
# deploy.sh's notify() uses); this script only ever forwards them to
# api.telegram.org, never logs or prints them.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_DIR=/var/lib/frozen-city-deploy
OFFSET_FILE="$STATE_DIR/tg-offset"

sudo mkdir -p "$STATE_DIR"
sudo chown ubuntu:ubuntu "$STATE_DIR"
[ -f "$OFFSET_FILE" ] || echo 0 >"$OFFSET_FILE"

if ! sudo test -r /etc/server-monitor/env; then
    echo "no /etc/server-monitor/env — cannot authenticate to Telegram, exiting" >&2
    exit 1
fi
# shellcheck disable=SC1091
source <(sudo cat /etc/server-monitor/env)
: "${TG_TOKEN:?TG_TOKEN missing from /etc/server-monitor/env}"
: "${TG_CHAT:?TG_CHAT missing from /etc/server-monitor/env}"

api() { curl -s --max-time 40 "https://api.telegram.org/bot${TG_TOKEN}/$1" "${@:2}"; }

reply() {
    api sendMessage \
        --data-urlencode "chat_id=${TG_CHAT}" \
        --data-urlencode "text=$1" \
        --data-urlencode "parse_mode=HTML" >/dev/null || true
}

# On a fresh start (no saved offset yet), drain whatever is already queued
# without acting on it — old backlog must never trigger a deploy.
if [ "$(cat "$OFFSET_FILE")" = "0" ]; then
    resp=$(api getUpdates --data-urlencode "offset=0" --data-urlencode "timeout=0") || resp=""
    last_id=$(echo "$resp" | jq -r '[.result[]?.update_id] | max // empty')
    [ -n "$last_id" ] && echo $((last_id + 1)) >"$OFFSET_FILE"
fi

echo "$(date -Is) listening for 'gitup' (long-poll getUpdates)..."

while true; do
    offset=$(cat "$OFFSET_FILE")
    resp=$(api getUpdates \
        --data-urlencode "offset=${offset}" \
        --data-urlencode "timeout=50" \
        --data-urlencode 'allowed_updates=["message"]') || {
        sleep 5
        continue
    }

    if [ "$(echo "$resp" | jq -r '.ok')" != "true" ]; then
        sleep 5
        continue
    fi

    while IFS= read -r upd; do
        [ -z "$upd" ] && continue
        update_id=$(echo "$upd" | jq -r '.update_id')
        echo $((update_id + 1)) >"$OFFSET_FILE"

        chat_id=$(echo "$upd" | jq -r '.message.chat.id // empty')
        text=$(echo "$upd" | jq -r '.message.text // empty')
        [ "$chat_id" = "$TG_CHAT" ] || continue

        shopt -s nocasematch
        if [[ "$text" =~ ^/?gitup$ ]]; then
            shopt -u nocasematch
            echo "$(date -Is) 'gitup' received, running deploy.sh"
            reply "🧊 Tekshirilmoqda..."
            BEFORE=$(git -C "$REPO_DIR" rev-parse HEAD)
            "$REPO_DIR/deploy.sh" || true
            AFTER=$(git -C "$REPO_DIR" rev-parse HEAD)
            if [ "$BEFORE" = "$AFTER" ]; then
                reply "✅ Allaqachon eng so'nggi versiyada (${AFTER:0:7})"
            fi
            # A real deploy's success/failure is already reported by
            # deploy.sh's own notify() — no need to duplicate it here.
        fi
        shopt -u nocasematch
    done < <(echo "$resp" | jq -c '.result[]?')
done
