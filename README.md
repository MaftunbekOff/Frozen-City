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

**Boshqaruv:** LMB — qurish/tanlash · RMB — bekor · 1–4 — tez qurish · WASD/strelkalar — kamera · g'ildirak — zoom · MMB — surish · Esc — bekor

## Arxitektura

```
src/game/   sof deterministik simulyatsiya (Bevy'siz) — testlanadi, WASM-tayyor
src/net/    TCP + in-memory kanallar; server thread 5 Hz tick, to'liq snapshot broadcast
src/client/ Bevy 0.19: protsedural render (assetlarsiz), UI, input
tests/      14 sim-invariant testi + 2 haqiqiy-TCP e2e test
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

Binar: `target/release/frozen_city(.exe)`. Multiplayer uchun hostning 4595/TCP porti ochiq bo'lsin.

## Testlar

```bash
cargo test          # 16 test: determinizm, invariantlar, protokol, 2-mijozli e2e
cargo run -- --smoke  # render smoke-test (avtomatik yopiladi)
```

## Yo'l xaritasi

WASM (WebSocket transport) · Android/iOS (touch) · chat · saqlash/yuklash (GameState allaqachon serde) · ko'proq binolar va texnologiya daraxti · delta-snapshot siqish · ovoz. Batafsil: [PLAN.md](PLAN.md).
