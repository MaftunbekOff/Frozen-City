# Frozen City

Muzlagan dunyoda omon qolish haqidagi **co-op shahar quruvchi o'yin** — Rust + [Bevy 0.19](https://bevy.org) da yozilgan, cross-platform (Windows / Linux / macOS) va multiplayer (TCP, server-avtoritativ).

A cooperative survival city-builder in the endless winter. Keep the furnace burning, feed your people, survive 12 days — alone or with up to 8 friends in one shared city.

![Genre](https://img.shields.io/badge/genre-survival%20city--builder-blue)
![Engine](https://img.shields.io/badge/engine-Bevy%200.19-orange)
![Multiplayer](https://img.shields.io/badge/multiplayer-co--op%20TCP-green)

## Ishga tushirish / Quick start

```bash
# O'ynash (menyu ochiladi)
cargo run --release

# Yakka o'yin darhol
cargo run --release -- --smoke      # 5 soniyalik render sinovi
cargo run --release                 # menyudan "Singleplayer"

# Multiplayer: bitta o'yinchi host bo'ladi
cargo run --release -- --host              # 4595-portda
# Do'stlar qo'shiladi:
cargo run --release -- --join 192.168.1.10 --name Aziz

# Oynasiz dedicated server (VPS uchun)
cargo run --release -- --server 4595
```

**Parametrlar:** `--name <ism>` · `--seed <n>` · `--days <n>` (standart 12) · `--host [port]` · `--join <ip[:port]>` · `--server [port]` · `--smoke`

## Brauzerda o'ynash (WASM)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <Cargo.lock'dagi wasm-bindgen versiyasi>
./build-web.sh                       # web/pkg ichiga yig'adi

cargo run --release -- --server 4595 # o'sha server web sahifani ham beradi
# Brauzerda: http://localhost:4595/
```

Dedicated server bitta 4595-portda uch xil trafikni gaplashadi: native TCP
mijozlar, brauzer WebSocket'lari va web-buildning statik fayllari (HTTP GET).
Brauzer va desktop o'yinchilar **bitta olamda** o'ynaydi.

**URL parametrlari (web):** `?name=Aziz` · `?server=wss://host/ws` · `?seed=7`
· `?days=20` · `?join` (menyusiz darhol serverga qo'shiladi). Sahifa qaysi
hostdan ochilgan bo'lsa, o'sha hostga ulanadi — standart yo'l `/ws` (reverse
proxy uchun qulay).

Brauzerda "Host" yo'q (sahifa port ochomaydi) — yakka o'yin sahifaning o'zida
ishlaydi (sim inline, xuddi shu deterministik yadro), multiplayer esa serverga
ulanish orqali.

## O'yin qoidalari

- Markazdagi **Pech (Furnace)** shaharning yuragi: ko'mir (yoki yog'och ×1.5) yeydi,
  atrofidagi radiusni isitadi. Daraja 0–3 — baland daraja = katta radius = ko'p yoqilg'i.
- Harorat har kuni pasayadi; tunlari ayoz, ba'zan **sovuq to'lqin** (oldindan e'lon qilinadi).
- Aholi och qolsa yoki sovuqda qolsa o'ladi. Chodirlar **issiqlik radiusi ichida** bo'lsin!
- Har kuni ertalab yangi omon qolganlar kelishi mumkin (bo'sh joy bo'lsa).
- **G'alaba:** belgilangan kungacha omon qolish. **Mag'lubiyat:** hamma o'lsa.

| Bino | Narx | Ishchi | Nima beradi |
|---|---|---|---|
| Chodir (Tent) | 15 yog'och | — | 4 kishiga boshpana |
| Arra zavodi (Sawmill) | 25 yog'och | 2 | 12 yog'och/kun/ishchi (yaqin o'rmondan) |
| Ko'mir koni (Coal Mine) | 30 yog'och | 3 | 15 ko'mir/kun/ishchi (kon ustiga quriladi) |
| Ovchi kulbasi (Hunter's Hut) | 25 yog'och | 2 | 10 oziq/kun/ishchi |

**Boshqaruv:** LMB — qurish/tanlash · RMB — bekor · 1–4 — tez qurish · WASD/strelkalar — kamera · Q/E — aylantirish · MMB — aylantirish/qiyalik · g'ildirak — zoom · Esc — bekor

O'yin **3D** (low-poly, protsedural): qiya 2.5D ko'rinish standart, lekin kamerani erkin aylantirish/egish mumkin. Kecha-kunduz haqiqiy yorug'lik bilan, pech esa atrofni yoritadi.

## Arxitektura

```
src/game/   sof deterministik simulyatsiya (Bevy'siz) — testlanadi, WASM'da ham ishlaydi
src/net/    TCP + WebSocket + in-memory kanallar; server thread 5 Hz tick, snapshot broadcast
            ws.rs — brauzer WebSocket transporti (wasm)
src/client/ Bevy 0.19: protsedural render (assetlarsiz), UI, input
            local_server.rs — brauzerda yakka o'yin (sim Bevy tizimi sifatida, threadsiz)
tests/      14 sim-invariant testi + 4 e2e test (TCP, WebSocket+TCP aralash, HTTP statik)
```

- **Server-avtoritativ:** mijozlar faqat buyruq yuboradi (`Place`, `Demolish`,
  `AdjustWorkers`, `SetFurnaceLevel`); server validatsiya qilib, holatni tarqatadi.
- **Yakka o'yin = xuddi shu server** in-process thread'da — alohida kod yo'li yo'q.
- Snapshot ~6 KB (teren 1 Hz'da to'liq keladi) — LAN va internet co-op uchun yetarli.
- Boshqa o'yinchilarning kursorlari va ismlari real vaqtda ko'rinadi.

## Cross-platform build

| Platforma | Buyruq | Eslatma |
|---|---|---|
| Windows | `cargo build --release` | tayyor |
| Linux | `cargo build --release` | `sudo apt install libasound2-dev libudev-dev libwayland-dev` |
| macOS | `cargo build --release` | Xcode CLT kifoya |
| Web | `./build-web.sh` | wasm32 target + wasm-bindgen-cli (+ ixtiyoriy binaryen) |

Binar: `target/release/frozen_city(.exe)`. Multiplayer uchun hostning 4595/TCP porti ochiq bo'lsin.

## Testlar

```bash
cargo test          # 18 test: determinizm, invariantlar, protokol, TCP/WS/HTTP e2e
cargo run -- --smoke  # render smoke-test (avtomatik yopiladi)
```

## Yo'l xaritasi

Android/iOS (touch) · akkauntlar + persistensiya (PostgreSQL) · ko'p region — bitta olam (gateway + region serverlar) · delta-snapshot + interest management · chat · ko'proq binolar va texnologiya daraxti · ovoz. Batafsil: [PLAN.md](PLAN.md).
