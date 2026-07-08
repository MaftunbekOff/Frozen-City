# FROZEN CITY — To'liq ishlab chiqish rejasi

**Janr:** Co-op survival city-builder (Frostpunk / Frozen City uslubida)
**Dvigatel:** Bevy 0.19 (Rust) — cross-platform: Windows, Linux, macOS
**Multiplayer:** Server-avtoritativ co-op (TCP), 1–8 o'yinchi bitta shaharni birga boshqaradi

---

## 1. O'yin kontsepsiyasi

Muzlagan dunyoda omon qolgan bir guruh odamlar markaziy pech (Furnace) atrofida
shahar quradi. Harorat kun sayin pasayadi. O'yinchilar (yakka yoki birgalikda):

- daraxt kesib **yog'och**, kon qazib **ko'mir**, ov qilib **oziq-ovqat** yig'adi;
- **chodirlar** qurib aholini sovuqdan asraydi;
- pechning yoqilg'isini va quvvat darajasini boshqaradi;
- yangi kelgan omon qolganlarni joylashtiradi va ishga tayinlaydi.

**G'alaba:** belgilangan kungacha (standart: 12-kun) omon qolish.
**Mag'lubiyat:** barcha aholi halok bo'lsa.

## 2. Texnologik stek va arxitektura

| Qatlam | Tanlov | Sabab |
|---|---|---|
| Dvigatel | Bevy 0.19 | Zamonaviy ECS, 2D/UI, barcha desktop platformalar |
| Serializatsiya | serde + bincode | Ixcham binary snapshot'lar |
| Tarmoq | std TCP + length-prefixed frames | Shahar quruvchi uchun ideal: ishonchli, tartibli, qo'shimcha dependency yo'q, versiya-mustaqil |
| RNG | O'zimizning SplitMix64 | Deterministik simulyatsiya, dependency'siz |

**Modullar (bitta crate):**

```
src/
  main.rs           — CLI args, Bevy App, rejimlar (menu / --host / --join / --server)
  game/
    types.rs        — GameState, Tile, Building, Survivor, PlayerCommand (serde)
    rng.rs          — SplitMix64 deterministik RNG
    sim.rs          — SOF simulyatsiya: map-gen, tick(), apply_command() — Bevy'siz
  net/
    protocol.rs     — ClientMsg/ServerMsg + 4-bayt length-prefix framing
    server.rs       — Sim thread (5 Hz) + TCP acceptor + har mijoz uchun reader/writer
    client.rs       — TCP yoki in-memory (local) ulanish, mpsc kanallar
  client/
    net_sync.rs     — Kanaldan snapshot qabul qilish, Bevy resursiga joylash
    render.rs       — Terrain/binolar/aholi/issiqlik radiusi/qor/kecha-kunduz
    input.rs        — Kamera pan+zoom, qurish rejimi, tanlash
    ui.rs           — HUD, qurish paneli, bino paneli, voqealar lentasi, game-over
    menu.rs         — Bosh menyu (Singleplayer / Host / Join)
```

**Muhim printsip:** `game/` moduli Bevy'ga umuman bog'lanmaydi — simulyatsiya sof
funksiya: `tick(&mut GameState)`. Bu unit-test qilishni, dedicated serverni va
kelajakda WASM'ni osonlashtiradi.

## 3. Tarmoq modeli (server-avtoritativ)

```
[Client A] --ClientMsg--> ┌────────────┐
[Client B] --ClientMsg--> │ SIM THREAD │ --GameState snapshot (5 Hz)--> hamma
[Local ]  --kanal orqali->│  5 Hz tick │
                          └────────────┘
```

- **Yagona haqiqat manbai — server.** Mijozlar faqat buyruq yuboradi
  (`Place`, `Demolish`, `AdjustWorkers`, `SetFurnaceLevel`, `Cursor`).
- Server har tickda buyruqlarni validatsiya qilib qo'llaydi, so'ng to'liq
  `GameState` snapshot'ini hammaga yuboradi (~6 KB; teren 1 Hz'da, ~25 KB).
- Yakka o'yin = xuddi shu server in-process thread'da, in-memory kanal orqali —
  bitta kod yo'li, alohida "singleplayer logikasi" yo'q.
- Rejimlar: `frozen_city` (menyu) · `--host [port]` · `--join <ip:port>` ·
  `--server [port]` (oynasiz dedicated server).
- Boshqa o'yinchilarning kursorlari va ismlari real vaqtda ko'rinadi (co-op his).

## 4. O'yin dizayni — balans jadvali

**Vaqt:** 1 o'yin kuni = 150 real soniya = 750 tick (5 Hz).

**Harorat:** `T = bazaviy(kun) + sutkalik ± sovuq to'lqin`
- bazaviy = −4°C − 1.2°C × kun (borgan sari sovuqlashadi)
- sutkalik: kunduzi +6°C, yarim tunda −6°C
- ~30% kunlarda tunda −10°C "sovuq to'lqin" (oldindan e'lon qilinadi)

**Binolar:**

| Bino | Narx (yog'och) | Ishchi | Ishlab chiqarish | Izoh |
|---|---|---|---|---|
| Pech (Furnace) | — | — | Issiqlik radiusi 10/16/22 tayl | 2×2, markazda, 1 dona |
| Chodir (Tent) | 15 | — | 4 kishi sig'im | Radius ichida bo'lsa isitiladi |
| Arra zavodi (Sawmill) | 25 | 2 | 12 yog'och/kun/ishchi | Yaqin o'rmonni kamaytiradi |
| Ko'mir koni (Coal Mine) | 30 | 3 | 15 ko'mir/kun/ishchi | Faqat ko'mir konida quriladi |
| Ovchi kulbasi (Hunter) | 25 | 2 | 10 oziq/kun/ishchi | — |

**Pech:** daraja 0–3. Sarf: 12 ko'mir/kun × daraja (ko'mir tugasa yog'och ×1.5).
**Aholi:** ochlik +100/kun (1.2 oziq/kun yeydi), sovuqda HP kamayadi, issiq va
to'q bo'lsa tiklanadi. Har 1–2 kunda yangi omon qolganlar keladi (sig'im bo'lsa).
**Boshlang'ich:** 8 kishi, 60 yog'och, 40 ko'mir, 25 oziq, pech 1-darajada.

## 5. Vizual uslub (assetlarsiz, protsedural)

- 2D top-down, 32px katakli grid; qor-oq teren, o'rmon, ko'mir koni ranglari.
- Binolar — rangli paneller + harf-ikonka + ishchi soni yorlig'i.
- Pech atrofida issiqlik radiusi — protsedural radial-gradient glow.
- Kecha-kunduz sikli — ekran ustidagi ko'k qorong'ilik qatlami.
- Yog'ayotgan qor zarralari (200 ta, klient tomonda).
- Aholi — kichik nuqtalar, o'z binosi atrofida kezib yuradi.

## 6. UI/UX

- **HUD (tepada):** yog'och/ko'mir/oziq, aholi (band/jami), kun+soat, harorat.
- **Qurish paneli (pastda):** 4 bino tugmasi (narxi bilan), 1–4 tezkor tugmalar.
- **Bino paneli:** binoni bosganda — ishchi +/−, buzish (40% qaytariladi).
- **Pech paneli:** daraja 0–3 tugmalari.
- **Voqealar lentasi (o'ngda):** "3 kishi keldi", "Sovuq to'lqin!", o'limlar.
- **Boshqaruv:** WASD/strelka — kamera, g'ildirak — zoom, LMB — qurish/tanlash,
  RMB/Esc — bekor qilish.

## 7. Bosqichlar (milestones)

1. **M0 — Skelet:** Cargo, CLI, Bevy oynasi ochiladi. ✅ shu sessiyada
2. **M1 — Sim yadrosi:** map-gen, tick, resurslar, harorat, o'lim/g'alaba + testlar. ✅
3. **M2 — Tarmoq:** protokol, server thread, TCP + local kanal, 2-mijoz e2e testi. ✅
4. **M3 — Render + input:** teren, binolar, kamera, qurish rejimi. ✅
5. **M4 — UI:** HUD, panellar, menyu, game-over. ✅
6. **M5 — Sayqal:** qor, kecha-kunduz, issiqlik glow, kursorlar. ✅
7. **M6 — Sifat:** unit + e2e testlar, multi-agent kod-tekshiruv, README. ✅

## 8. Sinov strategiyasi

- `cargo test`: sim invariantlari (10 000 tick panic'siz, resurslar ≥ 0),
  buyruq validatsiyasi, protokol framing round-trip.
- **E2E tarmoq testi:** testda haqiqiy server ko'tariladi, 2 ta TCP mijoz
  ulanadi, bino quradi, ikkalasi ham snapshot'da ko'rishini tasdiqlaydi.
- `--smoke` rejimi: oyna ochib ~4 soniyada avtomatik yopiladi (render sinovi).
- Multi-agent adversarial kod-tekshiruv (sim, tarmoq, Bevy API, UX).

## 9. Kelajak yo'l xaritasi (MVP'dan keyin)

- WASM (WebSocket transport + sim inline), Android/iOS (touch).
- Chat, pauza (yakka o'yinda), saqlash/yuklash (GameState allaqachon serde).
- Ko'proq binolar: kasalxona, oshxona, issiqxona, devor; texnologiya daraxti.
- Delta-snapshot siqish, interpolatsiya; Steam/relay orqali NAT-traversal.
- Ovoz effektlari va musiqa; sprite-art assetlar.
