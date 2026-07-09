# Frozen City

Muzlagan dunyoda omon qolish haqidagi **co-op shahar quruvchi o'yin** — Rust +
[Bevy 0.19](https://bevy.org) da yozilgan. **Desktop** (Windows / Linux /
macOS, native binary) + **brauzer** (WASM, WebGPU avtomatik WebGL2'ga
tushadigan zaxira bilan) + **mobil** (telefon brauzerida, touch boshqaruv va
past-quvvat grafika darajasi bilan) — hammasi **bitta umumiy olamda**
multiplayer (server-avtoritativ).

A cooperative survival city-builder in the endless winter. Keep the furnace burning, feed your people, survive 12 days — alone or with up to 8 friends in one shared city. Runs natively on desktop, in any browser (WebGPU with an automatic WebGL2 fallback), and on phones (touch controls).

![Genre](https://img.shields.io/badge/genre-survival%20city--builder-blue)
![Engine](https://img.shields.io/badge/engine-Bevy%200.19-orange)
![Multiplayer](https://img.shields.io/badge/multiplayer-co--op%20TCP-green)
![Platforms](https://img.shields.io/badge/platforms-Desktop%20%7C%20Web%20%7C%20Mobile-blueviolet)

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

## Brauzerda va mobilda o'ynash (WASM)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <Cargo.lock'dagi wasm-bindgen versiyasi>
./build-web.sh                       # web/pkg-webgpu VA web/pkg-webgl ikkalasini quradi

cargo run --release -- --server 4595 # o'sha server web sahifani ham beradi
# Brauzerda: http://localhost:4595/
```

`build-web.sh` **ikkita** wasm bundle quradi:

- `pkg-webgpu` — Bevy'ning tez, GPU-driven backend'i (zamonaviy brauzer + ishlaydigan GPU kerak)
- `pkg-webgl` — WebGL2, deyarli hamma qurilma/brauzerda ishlaydi (zaxira)

`web/boot.js` sahifa ochilganda **haqiqiy sinov** o'tkazadi
(`navigator.gpu.requestAdapter()` chaqirib) va mos bundle'ni yuklaydi.
Muhim: `navigator.gpu` obyektining borligi WebGPU ishlashini kafolatlamaydi
(GPU drayveri yo'q qurilmalar ham shu API'ni ko'rsatishi mumkin) — shuning
uchun faqat "feature bor-yo'qligini" emas, balki **haqiqiy adapterni** so'rab
ko'rish shart, aks holda mos kelmagan qurilmalarda o'yin sokin qulab tushadi.

Dedicated server bitta 4595-portda uch xil trafikni gaplashadi: native TCP
mijozlar, brauzer WebSocket'lari va web-buildning statik fayllari (HTTP GET,
ikkala bundle ham). Brauzer, mobil va desktop o'yinchilar **bitta olamda** o'ynaydi.

**Mobil (telefon):** alohida ilova/build shart emas — xuddi shu web sahifa,
lekin: touch boshqaruv (1 barmoq — pan/tanlash, 2 barmoq — zoom/aylantirish/
qiyalik), past-quvvat qurilmalar uchun grafika darajasi avtomatik pasayadi
(soyasiz, bloomsiz, piksel zichligi cheklangan — yuqori FPS uchun).

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
| Issiqxona (Greenhouse) | 35 yog'och | 2 | 13 oziq/kun/ishchi (yuqori-output ferma) |
| Kasalxona (Hospital) | 35 yog'och | 2 | ishlab tursa aholi HP'sini tiklaydi |
| Oshxona (Kitchen) | 25 yog'och | 1 | ishlab tursa shahar oziqni 25% tejaydi |

**Boshqaruv:** LMB — qurish/tanlash · RMB — bekor · 1–7 — tez qurish · WASD/strelkalar — kamera · Q/E — aylantirish · MMB — aylantirish/qiyalik · g'ildirak — zoom · Esc — bekor · **Enter — chat** · **Alt+klik — xaritaga ping**

O'yin **3D** (low-poly, protsedural): qiya 2.5D ko'rinish standart, lekin kamerani erkin aylantirish/egish mumkin. Kecha-kunduz haqiqiy yorug'lik bilan, pech esa atrofni yoritadi.

## Arxitektura

```
src/game/   sof deterministik simulyatsiya (Bevy'siz) — testlanadi, WASM'da ham ishlaydi
src/net/    TCP + WebSocket + in-memory kanallar; server thread 5 Hz tick, snapshot broadcast
            ws.rs — brauzer WebSocket transporti (wasm)
src/client/ Bevy 0.19: protsedural render (assetlarsiz), UI, input, chat, minimap, audio
            minimap.rs — butun xaritaning burchakdagi ko'rinishi (teren+binolar+kamera)
            audio.rs — protsedural WAV effektlari (qurish/voqea/bo'ron shamoli), assetsiz
            local_server.rs — brauzerda yakka o'yin (sim Bevy tizimi sifatida, threadsiz)
tests/      88 test, 9 faylda: sim (29) + rollar (17) + voqealar (10) + missiya (8)
            + texnologiya (8) + bino (4) + TCP/WS/HTTP e2e (4) + co-op e2e (4)
            + rollar e2e (4) — chat/attributsiya/reconnect/fuzz ham shular ichida
```

- **Server-avtoritativ:** mijozlar faqat buyruq yuboradi (`Place`, `Demolish`,
  `AdjustWorkers`, `SetFurnaceLevel`, `Chat`, `Ping`); server validatsiya qilib, holatni tarqatadi.
- **Yakka o'yin = xuddi shu server** in-process thread'da — alohida kod yo'li yo'q.
- Snapshot ~6 KB (teren 1 Hz'da to'liq keladi) — LAN va internet co-op uchun yetarli.
- Boshqa o'yinchilarning kursorlari va ismlari real vaqtda ko'rinadi.
- **Co-op (V0.2):** matnli chat, xaritaga ping (Alt+klik), «kim nima qurdi»
  attributsiyasi (`Building.owner` + hissa statistikasi), sessiya tokeni bilan
  avtomatik (fon-thread'da) qayta-ulanish, **rollar/egalik** (egasi mehmon
  huquqlarini belgilaydi: ko'rish / qurish / to'liq; mehmonni chiqarib yuboradi),
  va butun xaritaning burchakdagi **mini-xaritasi** (bosib borish mumkin).

## Cross-platform build

| Platforma | Buyruq | Eslatma |
|---|---|---|
| Windows | `cargo build --release` | tayyor |
| Linux | `cargo build --release` | `sudo apt install libasound2-dev libudev-dev libwayland-dev` |
| macOS | `cargo build --release` | Xcode CLT kifoya |
| Web (desktop brauzer) | `./build-web.sh` | wasm32 target + wasm-bindgen-cli (+ ixtiyoriy binaryen); WebGPU + WebGL2 ikkalasi ham quriladi |
| Mobil (telefon brauzeri) | `./build-web.sh` | alohida build shart emas — xuddi shu web build; touch boshqaruv va past-quvvat grafika darajasi avtomatik |

Native binar: `target/release/frozen_city(.exe)`. Multiplayer uchun hostning
4595/TCP porti ochiq bo'lsin. Rasmiy native mobil ilova (Android/iOS,
`cargo build --target ...`) hali reja bosqichida — hozircha mobil qo'llab-
quvvatlash **mobil brauzer** orqali (native ilovaga teng darajadagi
boshqaruv/unumdorlik bilan).

## Testlar

```bash
cargo test          # 88 test: determinizm, invariantlar, protokol+fuzz, e2e, rollar/missiya/binolar/texnologiya/voqealar
cargo run -- --smoke  # render smoke-test (avtomatik yopiladi)
```

## Yo'l xaritasi

**Vizyon:** shaxsiy olam (missiyalar) → **Tunnel** → Global Olam (butun dunyo bitta doimiy olamda) → do'stlarni o'z olamingga taklif qilish.

V0.2 tarmoq poydevori — ✅ chat · ✅ attributsiya · ✅ reconnect · ✅ rate-limit · ✅ rollar/egalik · ✅ minimap · V0.3 — ✅ missiyalar · ✅ Tunnel · ✅ 3 yangi bino · ✅ texnologiya daraxti · ✅ voqealar tizimi (kasallik/bo'ron/karvon-tanlov) · V0.4 akkauntlar + doimiy shaxsiy olamlar · V0.5 Global Olam (hub) · V0.6 taklif va mehmon co-op · V1.0 sayqal + tarqatish. Batafsil: [ROADMAP.md](ROADMAP.md).
