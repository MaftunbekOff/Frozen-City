#!/usr/bin/env python3
"""Frozen City registration bot.

Collects Username / Ism / Familiya / birth date over a short Telegram
conversation, then mints a Login + Parol the player uses to sign into the
game itself (see `ClientMsg::Login` in src/net/protocol.rs). Writes into the
same SQLite accounts DB the game server reads at
`/var/lib/frozen-city-accounts/accounts.db` (read-only there — this process
is the only writer).

Long-polls the Bot API `getUpdates` the same way `deploy-listen.sh` already
does for the deploy trigger, rather than pulling in a bot framework: this bot
has exactly one conversation shape, and plain `requests` + `sqlite3` covers
it without a new dependency to track.
"""

from __future__ import annotations

import os
import secrets
import sqlite3
import string
import sys
import time
import unicodedata
from datetime import date, datetime

import bcrypt
import requests

TOKEN = os.environ["TG_TOKEN"]
API = f"https://api.telegram.org/bot{TOKEN}"

DB_PATH = os.environ.get("FC_ACCOUNTS_DB", "/var/lib/frozen-city-accounts/accounts.db")
STATE_DIR = os.environ.get("FC_BOT_STATE_DIR", "/var/lib/frozen-city-bot")
OFFSET_FILE = os.path.join(STATE_DIR, "tg-offset")

PLAY_URL = "https://game.twelfth.uz"

LOGIN_ALPHABET = string.digits
# No ambiguous glyphs (0/O, 1/l/I) — this is read off a phone screen and
# retyped by hand.
PASSWORD_ALPHABET = "".join(c for c in string.ascii_letters + string.digits if c not in "0O1lI")

MIN_BIRTH_YEAR = 1920

# Per-chat conversation state, in memory only: a bot restart mid-form just
# means the player sends /start again, which is an acceptable reset for a
# form this short.
SESSIONS: dict[int, dict] = {}


def db() -> sqlite3.Connection:
    os.makedirs(STATE_DIR, exist_ok=True)
    os.makedirs(os.path.dirname(DB_PATH), exist_ok=True)
    conn = sqlite3.connect(DB_PATH)
    conn.execute("PRAGMA busy_timeout = 5000")
    conn.execute(
        """CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY,
            telegram_id INTEGER UNIQUE NOT NULL,
            telegram_username TEXT,
            display_username TEXT UNIQUE NOT NULL,
            first_name TEXT NOT NULL,
            last_name TEXT NOT NULL,
            birth_date TEXT NOT NULL,
            login TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        )"""
    )
    conn.commit()
    return conn


def api(method: str, **params) -> dict:
    # getUpdates asks Telegram to hold the connection open for `timeout`
    # (long-poll) seconds; the HTTP client's own timeout must exceed that or
    # every quiet period looks like a failed request instead of "no updates".
    poll_timeout = params.get("timeout", 0)
    resp = requests.post(f"{API}/{method}", data=params, timeout=poll_timeout + 10)
    resp.raise_for_status()
    return resp.json()


def send(chat_id: int, text: str) -> None:
    try:
        api("sendMessage", chat_id=chat_id, text=text, disable_web_page_preview=True)
    except requests.RequestException as e:
        print(f"[bot] sendMessage failed: {e}", file=sys.stderr)


def gen_login(conn: sqlite3.Connection) -> str:
    while True:
        login = "fc" + "".join(secrets.choice(LOGIN_ALPHABET) for _ in range(6))
        if conn.execute("SELECT 1 FROM accounts WHERE login = ?", (login,)).fetchone() is None:
            return login


def gen_password() -> str:
    return "".join(secrets.choice(PASSWORD_ALPHABET) for _ in range(10))


def hash_password(password: str) -> str:
    return bcrypt.hashpw(password.encode(), bcrypt.gensalt()).decode()


def parse_birth_date(text: str) -> date | None:
    normalized = text.strip().replace("/", ".").replace("-", ".")
    for fmt in ("%d.%m.%Y", "%d.%m.%y"):
        try:
            d = datetime.strptime(normalized, fmt).date()
        except ValueError:
            continue
        if d.year < MIN_BIRTH_YEAR or d > date.today():
            return None
        return d
    return None


def existing_account(conn: sqlite3.Connection, telegram_id: int) -> sqlite3.Row | None:
    conn.row_factory = sqlite3.Row
    return conn.execute(
        "SELECT * FROM accounts WHERE telegram_id = ?", (telegram_id,)
    ).fetchone()


def username_taken(conn: sqlite3.Connection, username: str) -> bool:
    return (
        conn.execute(
            "SELECT 1 FROM accounts WHERE display_username = ? COLLATE NOCASE", (username,)
        ).fetchone()
        is not None
    )


def finish_registration(conn: sqlite3.Connection, chat_id: int, telegram_id: int, tg_username: str | None) -> None:
    data = SESSIONS[chat_id]["data"]
    login = gen_login(conn)
    password = gen_password()
    password_hash = hash_password(password)
    try:
        conn.execute(
            """INSERT INTO accounts
                (telegram_id, telegram_username, display_username, first_name,
                 last_name, birth_date, login, password_hash, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                telegram_id,
                tg_username,
                data["username"],
                data["first_name"],
                data["last_name"],
                data["birth_date"].isoformat(),
                login,
                password_hash,
                datetime.utcnow().isoformat(timespec="seconds"),
            ),
        )
        conn.commit()
    except sqlite3.IntegrityError:
        # The username was taken by someone else between the availability
        # check and this insert (or, vanishingly unlikely, a telegram_id
        # double-registration race) — ask for a different username rather
        # than losing the rest of the form.
        SESSIONS[chat_id]["step"] = "username"
        send(
            chat_id,
            "Bu Username band bo'lib qoldi. Boshqa Username kiriting:",
        )
        return

    SESSIONS.pop(chat_id, None)
    send(
        chat_id,
        "Ro'yxatdan o'tish muvaffaqiyatli!\n\n"
        f"Login: {login}\n"
        f"Parol: {password}\n\n"
        "Ushbu login/parolni saqlab qo'ying — o'yinga shu orqali kirasiz.\n"
        f"O'ynash: {PLAY_URL}\n\n"
        "Parolni unutsangiz /parol buyrug'i bilan yangisini olishingiz mumkin.",
    )


def handle_start(conn: sqlite3.Connection, chat_id: int, telegram_id: int) -> None:
    row = existing_account(conn, telegram_id)
    if row is not None:
        send(
            chat_id,
            "Siz allaqachon ro'yxatdan o'tgansiz.\n\n"
            f"Login: {row['login']}\n\n"
            "Parolni unutgan bo'lsangiz /parol buyrug'ini yuboring.\n"
            f"O'ynash: {PLAY_URL}",
        )
        return
    SESSIONS[chat_id] = {"step": "username", "data": {}}
    send(
        chat_id,
        "Frozen City o'yiniga xush kelibsiz!\n\n"
        "Ro'yxatdan o'tish uchun bir nechta savolga javob bering.\n"
        "Istalgan vaqtda /bekor bilan bekor qilishingiz mumkin.\n\n"
        "Username kiriting (o'yin ichida ko'rinadigan ismingiz, 3-24 belgi):",
    )


def handle_parol(conn: sqlite3.Connection, chat_id: int, telegram_id: int) -> None:
    row = existing_account(conn, telegram_id)
    if row is None:
        send(chat_id, "Siz hali ro'yxatdan o'tmagansiz. /start buyrug'ini yuboring.")
        return
    password = gen_password()
    password_hash = hash_password(password)
    conn.execute(
        "UPDATE accounts SET password_hash = ? WHERE telegram_id = ?",
        (password_hash, telegram_id),
    )
    conn.commit()
    send(
        chat_id,
        f"Yangi parol yaratildi.\n\nLogin: {row['login']}\nParol: {password}\n\n"
        f"O'ynash: {PLAY_URL}",
    )


def handle_bekor(chat_id: int) -> None:
    if SESSIONS.pop(chat_id, None) is not None:
        send(chat_id, "Bekor qilindi. Qayta boshlash uchun /start yuboring.")
    else:
        send(chat_id, "Bekor qilinadigan narsa yo'q. /start bilan boshlashingiz mumkin.")


def handle_conversation(conn: sqlite3.Connection, chat_id: int, telegram_id: int, tg_username: str | None, text: str) -> None:
    session = SESSIONS[chat_id]
    step = session["step"]
    text = text.strip()

    if step == "username":
        cleaned = "".join(c for c in text if unicodedata.category(c) != "Cc")[:24].strip()
        if not (3 <= len(cleaned) <= 24):
            send(chat_id, "Username 3 dan 24 belgigacha bo'lishi kerak. Qayta kiriting:")
            return
        if username_taken(conn, cleaned):
            send(chat_id, "Bu Username band. Boshqasini kiriting:")
            return
        session["data"]["username"] = cleaned
        session["step"] = "first_name"
        send(chat_id, "Ismingizni kiriting:")
    elif step == "first_name":
        cleaned = text[:30]
        if not (2 <= len(cleaned) <= 30):
            send(chat_id, "Ism 2 dan 30 belgigacha bo'lishi kerak. Qayta kiriting:")
            return
        session["data"]["first_name"] = cleaned
        session["step"] = "last_name"
        send(chat_id, "Familiyangizni kiriting:")
    elif step == "last_name":
        cleaned = text[:30]
        if not (2 <= len(cleaned) <= 30):
            send(chat_id, "Familiya 2 dan 30 belgigacha bo'lishi kerak. Qayta kiriting:")
            return
        session["data"]["last_name"] = cleaned
        session["step"] = "birth_date"
        send(chat_id, "Tug'ilgan kun-oy-yilingizni kiriting (masalan: 15.03.2000):")
    elif step == "birth_date":
        parsed = parse_birth_date(text)
        if parsed is None:
            send(
                chat_id,
                "Sana noto'g'ri. DD.MM.YYYY ko'rinishida kiriting (masalan: 15.03.2000):",
            )
            return
        session["data"]["birth_date"] = parsed
        session["step"] = "confirm"
        d = session["data"]
        send(
            chat_id,
            "Tekshiring:\n"
            f"Username: {d['username']}\n"
            f"Ism: {d['first_name']}\n"
            f"Familiya: {d['last_name']}\n"
            f"Tug'ilgan sana: {parsed.strftime('%d.%m.%Y')}\n\n"
            "Tasdiqlash uchun 'ha' deb yozing, bekor qilish uchun /bekor.",
        )
    elif step == "confirm":
        if text.lower() in ("ha", "ha.", "xa", "yes"):
            finish_registration(conn, chat_id, telegram_id, tg_username)
        else:
            send(chat_id, "Tasdiqlash uchun 'ha' deb yozing, yoki /bekor bilan bekor qiling.")


def handle_update(conn: sqlite3.Connection, update: dict) -> None:
    msg = update.get("message")
    if not msg or "text" not in msg:
        return
    chat_id = msg["chat"]["id"]
    telegram_id = msg["from"]["id"]
    tg_username = msg["from"].get("username")
    text = msg["text"]

    if text.startswith("/start"):
        handle_start(conn, chat_id, telegram_id)
    elif text.startswith("/parol"):
        handle_parol(conn, chat_id, telegram_id)
    elif text.startswith("/bekor"):
        handle_bekor(chat_id)
    elif chat_id in SESSIONS:
        handle_conversation(conn, chat_id, telegram_id, tg_username, text)
    else:
        send(chat_id, "Ro'yxatdan o'tish uchun /start buyrug'ini yuboring.")


def read_offset() -> int:
    try:
        with open(OFFSET_FILE) as f:
            return int(f.read().strip() or "0")
    except (FileNotFoundError, ValueError):
        return 0


def write_offset(offset: int) -> None:
    tmp = OFFSET_FILE + ".tmp"
    with open(tmp, "w") as f:
        f.write(str(offset))
    os.replace(tmp, OFFSET_FILE)


def main() -> None:
    # The accounts DB holds password hashes; group (www-data, the game
    # server's own group) may read it, no one else. Set before any file gets
    # created — sqlite/open() honor the process umask, not a later chmod.
    os.umask(0o027)
    os.makedirs(STATE_DIR, exist_ok=True)
    conn = db()
    offset = read_offset()
    print(f"[bot] listening from offset {offset}...", flush=True)
    while True:
        try:
            resp = api(
                "getUpdates",
                offset=offset,
                timeout=50,
                allowed_updates='["message"]',
            )
        except requests.RequestException as e:
            print(f"[bot] getUpdates failed: {e}", file=sys.stderr)
            time.sleep(5)
            continue

        if not resp.get("ok"):
            time.sleep(5)
            continue

        for update in resp.get("result", []):
            offset = update["update_id"] + 1
            try:
                handle_update(conn, update)
            except Exception as e:  # noqa: BLE001 - keep the loop alive
                print(f"[bot] error handling update {update.get('update_id')}: {e}", file=sys.stderr)
            write_offset(offset)


if __name__ == "__main__":
    main()
