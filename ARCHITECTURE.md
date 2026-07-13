# Frozen City — Arxitektura

> Modul xaritasi. O'yin qoidalari va rivojlanish rejasi uchun: [README.md](README.md),
> [ROADMAP.md](ROADMAP.md), [PLAN.md](PLAN.md).

## 1. Umumiy ko'rinish

Kod uchta qatlamga bo'lingan, quyi qatlamlar yuqorisiga bog'liq emas. Bu
Cargo WORKSPACE: `game` va `net` alohida, jismoniy ajratilgan modul
crate'lari (`crates/fc-game`, `crates/fc-net`) sifatida yashaydi; ildiz paket
(`frozen_city`) ularni `src/lib.rs` orqali `pub use fc_game as game; pub use
fc_net as net;` bilan qayta eksport qiladi — shu sabab kod bazasidagi (va
testlardagi) barcha `frozen_city::game::...` / `frozen_city::net::...`
yo'llar o'zgarishsiz qoladi.

- **`crates/fc-game/src/`** (paket `fc-game`, crate nomi `fc_game`) — sof,
  deterministik simulyatsiya. Bevy'ga bog'liqlik yo'q: test qilinadi,
  dedicated serverda thread sifatida, brauzerda esa Bevy tizimi sifatida
  (threadsiz) ishlaydi. Yagona haqiqat manbai — `GameState`.
- **`crates/fc-net/src/`** (paket `fc-net`, crate nomi `fc_net`) — simni
  tarmoqqa chiqaradi: wire protokol (bincode + deflate), TCP/WebSocket/HTTP
  server, akkaunt autentifikatsiyasi, diskka saqlash (versiyalangan),
  per-akkaunt olam menejeri. `fc-game`ga bog'liq, Bevy'ga bog'liq emas.
- **`src/client/`** — Bevy 0.19 client: 3D protsedural render, UI/HUD, kirish,
  chat, minimap, audio — hammasi faqat `net`dan kelgan `GameState`
  snapshot'larini o'qiydi, o'zi hech qanday o'yin mantig'ini hisoblamaydi.

`src/main.rs` — CLI parsing + Bevy `App` qurish (native/wasm) yoki
`--server` rejimida to'g'ridan-to'g'ri `net::server`ni headless ishga tushirish.
`src/lib.rs` faqat `game` va `net`ni (`fc-game`/`fc-net` orqali) eksport
qiladi (`client` — binary-only modul, `main.rs` ichida).

### Oqim diagrammasi

```
                    ┌─────────────────────────┐
                    │   Brauzer (WASM klient)  │
                    │  Bevy render+UI+input    │
                    └─────────────┬────────────┘
                                  │ WebSocket (wss://.../ws)
                                  │
┌─────────────────────────┐      │      ┌─────────────────────────┐
│  Desktop klient (native) │      │      │  Boshqa o'yinchilar...   │
│  Bevy render+UI+input    │      │      └────────────┬────────────┘
└─────────────┬────────────┘      │                   │
              │ TCP (length-prefixed bincode)          │
              ▼                   ▼                    ▼
      ┌──────────────────────────────────────────────────────┐
      │        BITTA TCP PORT (4595) — protokol sniffing      │
      │  handle_socket: birinchi 4 bayt "GET " bo'lsa HTTP/WS,│
      │  aks holda native frame protokoli                     │
      │        (fc-net's server.rs: accept_loop → handle_*)     │
      └───────────────────────────┬────────────────────────────┘
                                  │ ToServer::{Join,Msg,Leave,...}
                                  ▼
      ┌──────────────────────────────────────────────────────┐
      │   sim_loop (alohida thread, 5 Hz = TICK_MS 200ms)      │
      │   1. navbatdagi ClientMsg'larni drain qilish            │
      │      (rate-limit, can_issue tekshiruvi)                │
      │   2. sim::apply_command + sim::tick                    │
      │   3. har tikda: ServerMsg::State snapshot broadcast     │
      │      (delta: tiles/events/chat/pings/missions/techs     │
      │       faqat o'zgarganda; buildings/survivors/stock      │
      │       har doim — deflate bilan siqilgan)                │
      └───────────────────────────┬────────────────────────────┘
                                  │
                                  ▼
                    GameState (yagona haqiqat manbai)
```

Guest (`Hello`) ulanishlari umumiy olamga (`sim_loop` — bitta process-wide
thread) boradi. Akkaunt bilan kirish (`Login`/`EnterCentral`) esa
`WorldManager` orqali **o'z alohida `sim_loop` thread'iga** yo'naltiriladi
(2-band, "Olam turlari"ga qarang).

---

## 2. Modullar

> **Katalog-modul konvensiyasi.** Eng katta uch modul — `types`, `sim`
> (fc-game) va `server` (fc-net) — domen bo'yicha submodullarga bo'lingan
> (Rust 2018 uslubi: `X.rs` modul-ildiz bo'lib qoladi, `mod Y; pub use Y::*;`
> orqali `X/Y.rs` submodullarni fasad qilib qayta eksport qiladi). Shu sabab
> butun kod bazasi ular ichidagi tiplarni **avvalgidek** `game::types::T`,
> `sim::tick`, `server::start` deb chaqiraveradi — tashqi API o'zgarmagan.
> Yangi tip/funksiya qo'shganda mos submodulga qo'ying; `pub(crate)`
> ko'rinishni fasad qayta eksportida ham `pub(crate)` saqlang.

### `crates/fc-game/src/` — sof simulyatsiya (Bevy'siz, paket `fc-game`)

| Fayl | Mas'uliyat | Asosiy tiplar/funksiyalar | Kim ishlatadi |
|---|---|---|---|
| `types/` (modul-ildiz `types.rs` + submodullar: `world`·`building`·`people`·`economy`·`progression`·`events`·`command`·`state`; konstantalar va `tile_index`/`in_bounds` ildizda) | Butun sim ma'lumot modeli: xarita, binolar, aholi, buyruqlar, ruxsat mantig'i | `GameState` (yagona haqiqat manbai — `tick`, `tiles`, `buildings`, `survivors`, `stock`, `players`, `phase`, `missions`, `tunnel`, `techs`, `central`, `graduated`, ...), `Survivor` (`hp`/`hunger`/`assigned_building`/`owner: Option<i64>` — akkaunt egaligi), `Building`, `PlayerInfo`, `PlayerCommand` (enum: `Place`/`Demolish`/`AdjustWorkers`/`AssignSurvivor`/`SetFurnaceLevel`/`InvestTunnel`/`Research`/`RespondEvent`), `GameState::can_issue` (buyruq ruxsatining **yagona** tekshiruv nuqtasi — server ham, sim ham, client UI ham shundan foydalanadi) | `sim.rs`, `fc-net` (`server.rs`, `protocol.rs`), butun `client/` |
| `sim/` (modul-ildiz `sim.rs` + submodullar: `mapgen`·`text`·`players`·`command`·`tick`) | Determinstik state-mashina: xarita generatsiyasi, tick mantig'i, buyruq qo'llash | `new_game(seed, win_days)` (shaxsiy/umumiy olam), `new_game_central(seed)` (markaziy olam — aholisiz, missiyasiz), `tick(state)` (har 200ms: ochlik/harorat/ishlab-chiqarish/voqealar — `central` bo'lsa ochlik/o'lim/g'alaba/voqealar o'tkazib yuboriladi), `apply_command(state, player, cmd)` (`can_issue`ni tekshirib bajaradi), `extract_migrants`/`inject_migrants` (Tunnel orqali ko'chish), `player_joined_as`/`player_rejoined`/`kick_player` | `fc-net`'s `server.rs` (`sim_loop`), `client/local_server.rs` (wasm inline), barcha `tests/*.rs` |
| `rng.rs` | Determinstik RNG | `Rng` (SplitMix64, `u64` state — `GameState.rng`/`event_rng` ichida saqlanadi, shu bilan butun sim bitta seed'dan takrorlanadi) | `sim.rs` (mapgen, voqealar), testlar |

### `crates/fc-net/src/` — protokol, server, persistensiya (paket `fc-net`, `fc-game`ga bog'liq)

| Fayl | Mas'uliyat | Asosiy tiplar/funksiyalar | Kim ishlatadi |
|---|---|---|---|
| `protocol.rs` | Wire format | `ClientMsg` (`Hello`/`Cmd`/`Cursor`/`Chat`/`Ping`/`SetGuestPermission`/`Kick`/`Login`/`EnterCentral`), `ServerMsg` (`Welcome`/`State`/`AuthFailed`), `Included` (delta-snapshot bayroqlari), `encode`/`decode` (bincode + `miniz_oxide` deflate), `write_frame`/`read_frame` (4-bayt little-endian uzunlik prefiksi). **MUHIM:** bincode positional bo'lgani uchun `ClientMsg`/`ServerMsg` enum variantlari **faqat OXIRIGA** qo'shiladi — o'rtaga qo'shish yoki tartibni o'zgartirish eski client/saqlovlarni buzadi | `server.rs`, `client.rs`, `ws.rs`, `legacy.rs` (saqlashda ham xuddi shu tamoyil) |
| `server/` (modul-ildiz `server.rs` + submodullar: `config`·`listener`·`native`·`web`·`ratelimit`·`messages`·`simloop`·`util`) | TCP/WS/HTTP qabul qilish + sim_loop + persistensiya orkestratsiyasi | `sim_loop` (asosiy tick tsikli — `pub(crate)`, `world_manager.rs` ham qayta ishlatadi), `accept_loop`/`handle_socket` (4-bayt sniffing: `"GET "` → HTTP/WS, aks holda native), `route_first_msg` (Hello/Login/EnterCentral marshrutlash), `RateLimiter` (Cmd 30/s, Chat 4/s, Ping 6/s, Cursor 60/s), sessiya tokenlari (`fresh_token()` — OS CSPRNG, har reconnect'da rotatsiya), `ServerConfig`/`ServerHandle`, `MAX_CONNECTIONS=128`, auto-reset (`WORLD_RESET_AFTER`), avtosaqlash (`AUTOSAVE_INTERVAL=20s`) | `main.rs` (`run_dedicated`), `client/menu.rs` (host/singleplayer in-process), `world_manager.rs` |
| `world_manager.rs` | Akkaunt → shaxsiy olam marshrutlash, lazy-spawn/idle-evict | `WorldManager::join_account` (login → akkauntning o'z `sim_loop` thread'i, birinchi so'rovda spawn qilinadi), `enter_central` (Tunnel bitirgan akkauntni markaziy olamga kiritadi, `extract_migrants`/`inject_migrants` orqali aholi ko'chiradi, `CENTRAL_MIGRANTS_PER_ACCOUNT=5`gacha), idle-evict 300s'dan keyin (`IDLE_SHUTDOWN`), `MAX_ACCOUNT_WORLDS=200` cap (markaziy olam bundan mustasno) | `main.rs` (`run_dedicated`, faqat asosiy region), `server.rs::route_first_msg` |
| `accounts.rs` | Akkaunt autentifikatsiyasi (SQLite, o'qish-only) | `authenticate(login, password)` → `(account_id, display_name)` yoki `None` (barcha xato holatlar — DB yo'q, login topilmadi, parol xato — bir xil `None`ga tushadi, enumeration oldini olish uchun), bcrypt tekshiruvi | `server.rs::route_first_msg` |
| `persist.rs` | Diskka saqlash/yuklash, versiyalangan format | `save_at`/`load_at` (`MAGIC_V2 = b"FCWORLD2"` header + bincode; atomik yozish: temp fayl + rename), magic'siz fayl `legacy.rs`dagi V1 ko'zgu orqali o'qiladi va migratsiya qilinadi | `server.rs` (`sim_loop`, `save_world`), `world_manager.rs`, `examples/checksave.rs` |
| `legacy.rs` | V1 (markaziy-olamgacha) format ko'zgusi — **hech qachon o'zgartirilmaydi** | `GameStateV1`/`SurvivorV1`/`PlayerInfoV1` (V1 layout aynan), `impl From<GameStateV1> for GameState` (yangi maydonlarga default: `owner: None`, `account: None`, `central: false`) | `persist.rs::load_at` (magic yo'q fayllar uchun) |
| `client.rs` | Client-tomon ulanish abstraktsiyasi | `ClientConn` enum (`Channels` — TCP pump thread yoki in-process kanal; `WebSocket` — faqat wasm), `connect_tcp`/`connect_tcp_with` (native TCP + reader/writer thread'lar), `poll()`/`send()` | `client/menu.rs`, `client/net_sync.rs`, `server.rs::connect_local` |
| `ws.rs` | Brauzer WebSocket transporti (**faqat wasm**) | `connect`/`connect_with` (`web_sys::WebSocket`, thread-local `SOCKETS` registr — JS qiymatlari `Send` emas), `send`/`is_closed`/`close` | `client.rs::ClientConn::WebSocket` (wasm branch) |
| `telemetry.rs` | Playtest telemetriyasi (server-tomon, native) — «kim, qancha vaqt, qay darajagacha o'ynadi» | `Event` (`SessionStart`/`SessionEnd` — oxirgisi progress snapshot bilan: kun/faza/graduated/binolar/aholi/missiya/tunnel/zaxira), `record` (env `FC_TELEMETRY_PATH` sozlagan process-keng `OnceLock` sink; o'rnatilmasa **no-op**, disk tegilmaydi), alohida yozuvchi thread JSONL append qiladi — tick **hech qachon** disk I/O'da bloklanmaydi | `server.rs::sim_loop` (Join → `session_start`, `disconnect`/kick → `session_end`) |

### `src/client/` — Bevy 0.19 render/UI/input

`mod.rs` — `ClientPlugin` (barcha kichik pluginlarni ro'yxatdan o'tkazadi,
`Screen` state-mashinasi: `Menu` ↔ `Game`), umumiy resurslar (`GameView` —
oxirgi snapshot ko'zgusi, `Session`, `NetConn`, `ServerRes`, `PendingSwitch`,
koordinata konvertatsiya funksiyalari `tile_center_world`/`world_to_tile`).

| Fayl | Mas'uliyat | Kim ishlatadi / bog'liqligi |
|---|---|---|
| `menu/` (modul: `layout`·`buttons`·`account`·`overlay`·`start`) | Bosh menyu: singleplayer/host/join, sof bosh-sahifa + Akkaunt/Sozlamalar **modal overlay**larда (`overlay.rs`); avtostart (`--host`/`--join`/`--smoke`) | `net::server` (host in-process), `net::client`/`net::ws` (join) |
| `net_sync.rs` | Snapshot qabul qilish → `GameView`ga ko'chirish, shaffof auto-reconnect (fon-thread'da qayta ulanish, sessiya tokeni bilan) | `NetConn`, `ClientConn::poll` |
| `render/` (modul: `components`·`assets`·`meshes`·`scene`·`terrain`·`buildings`·`survivors`·`cursors`·`effects`) | 3D protsedural sahna: teren, binolar, aholi, kecha-kunduz, pech yorug'i, qor, kursorlar — hammasi assetsiz | `GameView` (snapshot o'qish), `Quality` (grafika darajasi) |
| `input.rs` | Kamera boshqaruvi, qurish input'i, tanlash | `BuildMode`, `Selection`, `PlayerCommand` yuborish |
| `touch.rs` | Mobil touch: pan/tap/pinch/twist/tilt | `input.rs`ning touch ekvivalenti |
| `ui/` (modul: `components`·`hud`·`buildbar`·`selection`·`gameover`) | HUD, qurish paneli, tanlash paneli, game-over ekrani | `GameView`, `PlayerCommand` |
| `chat.rs` | Matnli chat (Enter bilan ochiladi) | `GameState.chat`, `ClientMsg::Chat` |
| `minimap.rs` | Burchak mini-xaritasi (CPU teksturaga bake) | `GameView`, `RelativeCursorPosition` (bosib borish) |
| `roles.rs` | Rollar/egalik paneli: `GuestPermission` sozlash, kick | `ClientMsg::SetGuestPermission`/`Kick` |
| `roster/` (modul: `components`·`panel`·`card`) | Aholi ro'yxati modali, `AssignSurvivor` | `ClientMsg::Cmd(AssignSurvivor)` |
| `social/` (modul: `components`·`spawn`·`panel`·`friends`·`toast`) | Do'stlar/taklif/tashrif paneli, yaqin-atrof chat pufakchalari, tashrif siyosati | `ClientMsg` (Invite/VisitFriend/…), `Social` snapshot |
| `missions.rs` | Missiya va Tunnel paneli | `ClientMsg::Cmd(InvestTunnel)` |
| `research.rs` | Texnologiya daraxti modali (`R` tugmasi) | `ClientMsg::Cmd(Research)` |
| `events.rs` | Karvon-tanlov popup + kasallik/bo'ron status | `ClientMsg::Cmd(RespondEvent)` |
| `audio.rs` | Protsedural WAV sintez (qurish/voqea/bo'ron shamoli) | `bevy_audio`, assetsiz |
| `local_server.rs` | **Faqat wasm**: brauzer singleplayer — sim Bevy tizimi sifatida inline (threadsiz), xuddi shu `ClientConn` interfeysini taqlid qiladi | `game::sim` to'g'ridan-to'g'ri (server orqali emas) |

---

## 3. Olam turlari va o'yinchi sayohati

Uchta olam turi, barchasi bitta `GameState`/`sim` mexanizmidan foydalanadi:

1. **Mehmon umumiy olami** — `Hello` (akkauntsiz) bilan kiriladi. Har region
   (asosiy + region2 + region3 — 3 mustaqil static process/port) o'zining
   bitta umumiy `sim_loop`ini yuritadi (`ServerConfig::persistent=true`,
   `save_path: None` → `persist::save`/`load`, `FC_WORLD_SAVE`). G'alaba/
   mag'lubiyatdan so'ng `WORLD_RESET_AFTER=45s`dan keyin yangi xarita bilan
   qayta boshlaydi (o'yinchilar ulanishda qoladi).
2. **Shaxsiy akkaunt-olam** — `Login` bilan kiriladi (asosiy regionda,
   `/ws`da; region2/3 `FC_DISABLE_ACCOUNTS=1` bilan rad etadi).
   `WorldManager` `account_id` bo'yicha alohida `sim_loop` thread'ini
   lazy-spawn qiladi (`/var/lib/frozen-city/accounts/{id}.bin`), 300s
   faolsizlikdan keyin avto-evict+save. Missiyalar, Tunnel, texnologiya
   daraxti — shu yerda progressiya. Tunnel bitgach `graduated = true`
   (doimiy, keyingi world-reset'larda ham saqlanadi).
3. **Markaziy olam (Global Olam)** — `EnterCentral` bilan, faqat Tunnel
   bitirgan (`graduated`) akkauntlar. Bitta doimiy `sim_loop`
   (`CENTRAL_KEY = -1` maxsus kalit, `central.bin`). `GameState.central =
   true`: ochlik/o'lim/g'alaba/mag'lubiyat/voqealar yo'q. Kirishda shaxsiy
   olamdan `CENTRAL_MIGRANTS_PER_ACCOUNT=5`tagacha aholi Tunnel orqali
   ko'chib o'tadi (`extract_migrants`/`inject_migrants`) va
   `Survivor::owner: Option<i64>` orqali faqat o'z akkauntiga bog'lanadi —
   `can_issue`ning `central` tarmog'i har kim faqat **o'z** ko'chmanchilarini
   boshqarishini ta'minlaydi (Owner/Guest rol tizimi bu yerda ishlamaydi).

**Sayohat:** yangi o'yinchi akkauntsiz shaxsiy/umumiy olamda boshlaydi →
missiyalarni bajarib Tunnelni quradi → graduatsiya g'alabasi (akkaunt shu
bosqichda yaratiladi, Telegram bot orqali) → `EnterCentral` bilan Global
Olamga o'tadi, u yerda boshqa o'yinchilar bilan uchrashadi → (kelajakda,
V0.6) do'stlarini o'z shaxsiy olamiga taklif qiladi.

---

## 4. Doimiy tamoyillar

(To'liq ro'yxat: [ROADMAP.md](ROADMAP.md) "Doimiy tamoyillar" bo'limi.)

1. `crates/fc-game/` Bevy'siz qoladi — sim sof funksiya, WASM'da ham ishlaydi.
2. Determinizm buzilmaydi — har mexanika `--seed` bilan takrorlanadi.
3. Yakka o'yin = xuddi shu server (in-process, alohida kod yo'li yo'q).
4. Shaxsiy olam sim'i arzon qolsin — minglab olam parallel ishlashi kerak.
5. Har feature test bilan keladi (sim-invariant yoki e2e).
6. Bitta port printsipi — TCP + WS + HTTP birgalikda, deploy sodda.
7. **`src/client/`ga tegadigan har o'zgarish smoke-test bilan tasdiqlanadi.**
   `cargo test` faqat `fc-game`/`fc-net` sof mantiqni tekshiradi — Bevy
   client (ECS/render) hech qachon haqiqatda ishga tushirilmaydi, shuning
   uchun runtime-only xatolar (masalan ECS query-conflict) testlardan sizib
   o'tishi mumkin. Shu sabab `deploy.sh`da majburiy bosqich bor:
   `xvfb-run -a timeout 150 target/release/frozen_city --smoke` — muvaffaqiyat-
   siz bo'lsa deploy web-build bosqichigacha yetib bormay to'xtaydi.

**Saqlash-format migratsiya qoidasi:** `GameState`ga yangi maydon qo'shishdan
OLDIN — yangi versiya magic (`persist.rs`, hozir `FCWORLD5`) + `fc-net`ning `legacy.rs`
zanjiriga yangi bo'g'in (eski struktura muzlatilgan holicha qoladi, `From`
impl bilan migratsiya) qo'shilishi shart, va deploy'dan oldin haqiqiy
production saqlovlar nusxasi `examples/checksave.rs` bilan tekshiriladi.

---

## 5. Infratuzilma

### systemd xizmatlari

| Xizmat | Vazifasi | Port | Eslatma |
|---|---|---|---|
| `frozen-city` | Asosiy region — umumiy olam + akkauntlar/markaziy olam | 4595 | `/ws`; `--days 60`, `RuntimeMaxSec=10800`, `FC_TELEMETRY_PATH` |
| `frozen-city-region2` | Qo'shimcha static region | 4596 | `/ws-r2`; `FC_DISABLE_ACCOUNTS` yo'q lekin server kodi region2/3'da hech qachon `world_manager` bilan ishga tushmaydi — faqat asosiy `main.rs::run_dedicated` chaqiruv nuqtasi orqali |
| `frozen-city-region3` | Qo'shimcha static region | 4597 | `/ws-r3` |
| `frozen-city-bot` | Telegram ro'yxatdan o'tish boti (`bot/register_bot.py`) | — | SQLite accounts DB yagona yozuvchisi (`/var/lib/frozen-city-accounts/accounts.db`) |
| `frozen-city-deploy-listen` | Telegram'da "gitup" xabarini kutadi, `deploy.sh`ni ishga tushiradi | — | Pollingsiz — faqat so'rov bo'yicha deploy |

### nginx marshrutlar (`game.twelfth.uz` va `twelfth.uz/game/`)

- `/ws` → `127.0.0.1:4595` (asosiy region, WebSocket proxy, `proxy_read_timeout 4h`)
- `/ws-r2` → `127.0.0.1:4596` (region2)
- `/ws-r3` → `127.0.0.1:4597` (region3)
- `/` (yoki `/game/`) → statik web build (`gzip_static on`, `pkg-webgpu`/`pkg-webgl` keshsiz)

### Deploy oqimi (`deploy.sh`)

1. `git fetch` + fast-forward tekshiruvi (divergent bo'lsa to'xtaydi).
2. `cargo test --release` — muvaffaqiyatsiz bo'lsa to'xtaydi, jonli servis tegilmaydi.
3. `cargo build --release` — native binary.
4. **Majburiy smoke-test**: `xvfb-run -a timeout 150 target/release/frozen_city --smoke`
   — panik yoki non-zero exit bo'lsa to'xtaydi (3-band, tamoyil 7ga qarang).
5. `./build-web.sh` — web bundle (eng uzun bosqich, ~5-9 daqiqa).
6. `systemctl stop frozen-city` → binary+web nusxalash `/opt/frozen-city`ga →
   `systemctl start frozen-city`.
7. Har bosqichda Telegram'ga bildirishnoma (`notify()`).

`deploy-listen.sh` Telegram'dagi "gitup" xabarini uzoq-poll qiladi va
`deploy.sh`ni chaqiradi — GitHub'ni davriy so'rash yo'q.

### `build-web.sh` — ikkita wasm bundle

- `pkg-webgpu` (`--features webgpu`) — Bevy'ning tez GPU-driven backend'i.
- `pkg-webgl` (default features) — WebGL2, deyarli har qanday qurilmada ishlaydi.

Ikkalasi ham `wasm-opt -Oz` bilan siqiladi (agar binaryen o'rnatilgan bo'lsa)
va `gzip -9` bilan oldindan siqilgan `.gz` nusxa yaratiladi (`web/boot.js`
sahifa ochilganda haqiqiy `navigator.gpu.requestAdapter()` sinovi bilan
mosini tanlaydi — shunchaki `navigator.gpu` borligini emas).

### Playtest telemetriyasi (`FC_TELEMETRY_PATH`)

Server har ulanishga ikki voqea yozadi — join'da `session_start`, ketishda
progress-snapshot bilan `session_end` (`crates/fc-net/src/telemetry.rs`). Bu
bizga **haqiqiy o'yinchini o'lchash** imkonini beradi (shu paytgacha barcha
raqam botlardan edi): DAU/bir vaqtdagi cho'qqi, sessiya uzunligi, **qaysi
o'yin-kunida tashlab ketishadi** (survival o'yin uchun eng muhim retention
signali) va vizyon-funneli (Tunnel → graduatsiya → Global Olam).

- **Yoqish:** systemd unit'ida `Environment=FC_TELEMETRY_PATH=/var/lib/frozen-city/telemetry.jsonl`.
  O'rnatilmasa telemetriya **butunlay no-op** (test/singleplayer/`--host` disk tegmaydi).
- **Dizayn:** `persist`/`accounts` kabi process-keng, env bilan sozlanadigan
  yagona sink — shuning uchun `ServerConfig`ga, `start()`ga yoki testlarga
  tegmaydi. Barcha `sim_loop`'lar (umumiy, har akkaunt, markaziy) bitta
  kanalga voqea tashlaydi; alohida yozuvchi thread JSONL append qiladi —
  tick hech qachon disk I/O'da bloklanmaydi.
- **O'qish:** `python3 bot/analyze_telemetry.py [/var/lib/frozen-city/telemetry.jsonl]`
  — matn hisobot (kunlik faollik, sessiya uzunligi persentillari, tashlab-
  ketish-kun gistogrammasi, funnel). Batafsil playtest protokoli: [PLAYTEST.md](PLAYTEST.md).
