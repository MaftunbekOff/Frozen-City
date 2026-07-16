# FROZEN CITY — Yo'l xaritasi (Roadmap)

> Dolzarb rivojlanish rejasi. Dastlabki dizayn-hujjat: [PLAN.md](PLAN.md).
> Yangilangan: 2026-07-12.

## Hozirgi holat (V0.1–V0.7 — barchasi tayyor; keyingisi V1.0)

**2026-07-12 (kech):** V0.7 — aholi boshqaruvi — yakunlandi (rejaga keyin
qo'shilgan bosqich, batafsili "V0.7" bo'limida): aholini tanlab yurgizish
(`MoveSurvivor`), yetakchi tayinlash (`SetLeader`, bonus/motam), 6 kasb,
XP/darajalar, koloniya morale, roster'da tafsilot kartasi. Saqlov FCWORLD4
(V3→V4 migratsiya). 6 yangi test-fayl; test/clippy/wasm/smoke to'rttalasi toza.

**2026-07-12:** V0.4, V0.5 va V0.6 to'liq yakunlandi (batafsili har bo'limda):
client-ichidan ro'yxatdan o'tish, social panel (do'stlar/taklif/tashrif),
yaqin-atrof chat pufakchalari, markaziy olamda yengil avatar-rejim, akkaunt-
asosli bino egaligi + hissa daftari (FCWORLD3 migratsiya bilan), do'stlar
vitrinasi (Showcase), egasi-oflayn mehmon siyosati, taklifning shaxsiy olamga
ham yetib borishi. Barcha qabul mezonlari testlarda o'lchab tasdiqlangan
(50 parallel olam, 100 klient bitta hub'da, o'tish 0.8–0.9s). Loyiha Cargo
workspace modullariga ajratildi: `crates/fc-game` (sof sim), `crates/fc-net`
(protokol/server), root `frozen_city` (Bevy klient) — [ARCHITECTURE.md](ARCHITECTURE.md).

M0–M7 bosqichlar yakunlangan:

- ✅ Deterministik sim yadrosi (`src/game/`, Bevy'siz) + invariant testlar
- ✅ Server-avtoritativ co-op: TCP + WebSocket + HTTP bitta portda, 1–8 o'yinchi
- ✅ To'liq 3D low-poly protsedural render, kecha-kunduz, issiqlik glow
- ✅ HUD, qurish/bino/pech panellari, menyu, voqealar lentasi, game-over
- ✅ Brauzer (WASM), URL parametrlari, inline singleplayer
- ✅ Mobil touch boshqaruv va grafika sifat darajalari
- ✅ 101 test: sim invariantlari + attributsiya/chat/reconnect/rollar/missiya/bino/texnologiya/voqea/akkaunt + fuzz
- ✅ Mini-xarita (minimap): butun xaritaning burchakdagi ko'rinishi + bosib borish + pinglar
- ✅ V0.3: **missiyalar**, **Tunnel** (graduatsiya), 3 yangi **bino**,
  **texnologiya daraxti** (5 tech), **voqealar tizimi** (kasallik/bo'ron/karvon-tanlov)
- ✅ **Mobil-web unumdorlik** (V1.0 poydevori): server **gzip** beradi (wasm
  66MB→15MB, 4.4×) + cache header, DPR cap + geometriya/yorug'lik kamaytirish,
  **umumiy bino materiallari** (har bino turi bitta material = kamroq
  draw-call), FPS diagnostika HUD'da
- ✅ **WebGPU + WebGL2 fallback (ikkalasi ham quriladi)**: `build-web.sh` ikkita
  bundle chiqaradi (`pkg-webgpu`, `pkg-webgl`); `boot.js` sahifa ochilganda
  **haqiqiy** `navigator.gpu.requestAdapter()` sinovi bilan mosini tanlaydi —
  `navigator.gpu` obyekti borligi hali adapter ishlashini kafolatlamaydi
  (GPU drayversiz qurilmalar shu tuzoqqa tushib qulab tushardi — production'da
  jonli tasdiqlangan va tuzatilgan). HUD FPS qatorida joriy backend ko'rinadi.
- ✅ **Graduatsiya g'alabasi ajratildi**: Tunnel bitgani (Global Olamga chiqish)
  endi kun-omon-qolish g'alabasidan alohida ekranda ko'rsatiladi (`graduated` bayrog'i)
- ✅ **Arxitektura auditi** (55 tekshirilgan topilma): WebGL2'dan tashqari cheklovlar
  aniqlandi; har biri file:line bilan tuzatilgan/qisman/ochiq holatga tushirildi
- ✅ **Xavfsizlik va robustness qattiqlashtirish** (auditdan):
  **CSPRNG sessiya tokenlari** (`getrandom` — teskari SplitMix64 o'rniga: token-o'g'irlash/egalik-egallash yopildi),
  **ulanish chegarasi** (MAX_CONNECTIONS=128 + Drop-guard: thread-flood DoS),
  **WS kadr limiti** 8MB'ga moslashtirildi (64MB o'rniga),
  **ism-sanitayzer** (bidi/zero-width/zalgo endi ismlarda ham),
  **koordinata validatsiya** (NaN/inf + xarita chegarasi)
- ✅ **Ikkinchi unumdorlik o'tishi**: **wasm-opt** (binaryen) yig'ishga qo'shildi (wasm o'lchami),
  umumiy kursor materiallari, furnace ishlatilmagan handle, HUD/FPS o'zgarishda-yangilash,
  minimap pixel-kvantlangan re-upload; **rust-toolchain pin** + gzip qamrovi kengaytirildi

**V0.2 — ✅ to'liq bajarildi:** chat, attributsiya, reconnect, rate-limit, rollar/egalik,
delta-snapshot + siqish va interpolatsiya — barchasi yakunlandi (batafsili pastda, "V0.2" bo'limida).

**V0.3 — ✅ to'liq bajarildi:** missiyalar, texnologiya daraxti, voqealar tizimi, Tunnel
(graduatsiya g'alabasi), 4 → 8 bino (batafsili pastda, "V0.3" bo'limida); ochiq qolgani
faqat balans-regressiya testlari sonini oshirish.

**Rasmiy rejadan tashqari, allaqachon qurilgan va production'da ishlayotgan** (V0.4/V0.5'ning
zaminini tashkil qiladi, lekin ularning to'liq ko'zlangan shaklidan hali farq qiladi —
tafsilotlar "V0.4" bo'limida):
- **Akkaunt + login**: Telegram bot orqali ro'yxatdan o'tish (`bot/register_bot.py`,
  bcrypt), server `ClientMsg::Login`/`AuthFailed` bilan tekshiradi, reconnect akkaunt
  bilan kirganda `Login`ni qayta jo'natadi.
- **Dunyo persistensiyasi**: guest'lar uchun hamon bitta umumiy `world.bin` (bincode),
  har 20s avtosaqlash + SIGTERM handler. Akkaunt bilan kirganlar uchun endi
  **har akkauntga alohida** olam va fayl (`world_manager.rs`, "V0.4" bo'limida).
- ~~**Ko'p-region infratuzilmasi**~~ — **olib tashlandi (2026-07-16)**: 3ta
  mustaqil static olam (asosiy + region2 + region3, alohida systemd xizmat va
  portlarda), brauzerda region tanlash menyusi va `FC_DISABLE_ACCOUNTS` gate'i
  butunlay olib tashlandi (kod, jonli systemd/nginx marshrutlari va saqlangan
  fayllar bilan birga) — endi bitta jarayon, bitta port. PWA (manifest+service
  worker) va yuk-test vositasi (`examples/loadtest.rs`) region'ga bog'liq
  emasligi sababli qoldi.
- **Markaziy olam (V0.5'ning birinchi bosqichi, 2026-07-11)**: Tunnel bitgan akkaunt
  `EnterCentral` bilan bitta doimiy **Global Olamga** o'tadi; o'tishda shaxsiy
  olamidan 5 tagacha aholi **ko'chib o'tadi** (Tunnel orqali, shaxsiy olamdan
  chiqib ketadi) va markaziy olamda **faqat egasi boshqaradigan** ko'chmanchilarga
  aylanadi (`Survivor::owner`, akkaunt bo'yicha). Markaziy olamda ochlik/o'lim/
  g'alaba/mag'lubiyat yo'q — doimiy uchrashuv maydoni. Saqlash formati
  versiyalandi (`FCWORLD2` + V1 migratsiya, `net/legacy.rs`) — eski production
  olamlar buzilmasdan o'qiladi.

**Hal qilingan follow-up'lar (avvalgi review'dan):**
- ✅ Async reconnect — fon thread'ida dial, ilova muzlamaydi.
- ✅ Socket write timeout — sekin/o'lik mijoz endi ~30s'da uziladi (kanal o'smaydi).
- ✅ Voqealar lentasi — tizim voqealari (o'lim/ob-havo) player-spam bilan o'chmaydi.
- ✅ Chat "zalgo" — ketma-ket combining-mark'lar cheklandi (bidi/zero-width ham).

**Ikkinchi review (rollar/minimap) natijasida tuzatilgan:** egalik yagonaligi
(`owner_id` — parked egasini begona egallay olmaydi, ikki egadan himoya); navbatdagi
buyruq ruxsat-bypass'i (uzilgan/kicked o'yinchi buyrug'i tashlanadi); mini-xarita
bosish o'lik kod edi (UiGlobalTransform → RelativeCursorPosition); HUD tizim-voqealariga
ustuvorlik; reconnect token-rotatsiyasi (sniffing himoyasi); guest_perm reset-omon-qolishi;
zalgo tartibi/diapazonlari; mini-xarita per-frame GPU yuklamasi.

**Qolgan ochiq ishlar (auditdan, kelajakdagi bosqichlar):**
- **Tarmoq (arxitekturaviy):** ✅ delta-snapshot + WS/TCP kadr siqilishi (2026-07-10,
  V0.2'ga qarang) · **bounded** chiquvchi/kiruvchi navbat qoldi (hozir 30s
  write-timeout + drain-cap qisman himoya).
- **Moderatsiya/egalik:** **ban ro'yxati** (kicked mehmon qaytadi) · **owner-transfer**
  (egasi butunlay ketsa). Eslatma: barcha ulanishlar nginx orqali proxy qilingani
  uchun kelib chiqish IP `X-Forwarded-For`'da bor, lekin serverning o'zi hali undan
  foydalanmaydi — per-IP cheklov/ban ishlamaydi, akkaunt-identity (V0.4) kerak.
- **Xavfsizlik (transport):** ✅ TLS/wss — nginx `game.twelfth.uz` uchun Let's Encrypt
  sertifikat bilan HTTPS/WSS beradi (avto-yangilanadi, `certbot`), `/ws` → `127.0.0.1:4595`
  proxy; origin (4595-port) tashqariga ochilmagan.
- **Web hajmi:** **bevy feature-trim** · release `opt-level="z"` (wasm) — WebGL2
  fallback build ✅ bajarildi (yuqoriga qarang), qolgani hajm optimizatsiyasi.
- **Kichik:** bincode **varint** · terrain **indexed mesh** · temperature() `cos`
  cross-platform seed-repro · MIN_WIN_DAYS'ni `new_game`da ham clamp qilish.

## Katta maqsad (Vision) — o'yinchi sayohati

```
┌──────────────────┐   Tunnel    ┌──────────────────┐   taklif   ┌──────────────────┐
│  SHAXSIY OLAM    │ ──────────> │   GLOBAL OLAM    │ ─────────> │  SHAXSIY OLAM    │
│  o'z shahring,   │   quriladi  │  butun dunyo     │  do'stlar  │  endi mehmonlar  │
│  missiyalar,     │             │  bitta doimiy    │            │  bilan birga     │
│  progressiya     │             │  olamda yuradi   │            │  quriladi        │
└──────────────────┘             └──────────────────┘            └──────────────────┘
```

1. O'yinchi **shaxsiy olamida** boshlaydi — shahar quradi, **missiyalarni** bajaradi,
   progressiya orqali yangi binolar/texnologiyalar ochadi. Akkaunt shart emas —
   darhol o'ynay boshlaydi.
2. Progressiya cho'qqisi — **Tunnel**: katta, ko'p bosqichli qurilish loyihasi.
   Tunnel bitgach Global Olamga yo'l ochiladi (shu nuqtada akkaunt yaratiladi).
3. **Global Olam** — butun dunyo o'yinchilari bitta doimiy olamda: avatar bilan
   yurish, uchrashish, chat, do'stlashish.
4. Global Olamda topilgan do'stlarni o'yinchi **o'z shaxsiy olamiga taklif qiladi** —
   shaxsiy shahar mehmonlar bilan co-op'ga aylanadi, lekin olam egasi kim ekani
   va huquqlar aniq.

**Nega bu arxitektura to'g'ri:** og'ir shahar-simulyatsiya har o'yinchining alohida
olamida qoladi (arzon, parallel, shardlash oson); faqat hub umumiy — u yerda esa
sim emas, avatar+chat bor. Shuning uchun minglab o'yinchiga masshtablanadi.

Bu **onlayn o'yin**: barcha saqlash server tomonida avtomatik, o'yinchiga
saqlash menyusi yo'q; pauza yo'q — olamlar yashayveradi.

---

## V0.2 — Tarmoq va co-op poydevori

**Maqsad:** internet orqali barqaror o'yin + «kim nima qildi» aniq bo'lishi.
Butun ijtimoiy tsikl (mehmonlar, hub) shu poydevorga quriladi.

### Vazifalar

- [x] **Chat**: matnli xabarlar (Enter) + xaritada ping/marker qo'yish (Alt+klik).
      Chat va pinglar snapshot ichida (`GameState.chat` / `.pings`), server orqali
      tarqaladi; pinglar simda TTL bilan o'chadi (deterministik).
- [x] **Attributsiya**: `apply_command` o'yinchi id'sini ishlatadi —
      `Building.owner`, voqealar lentasida «Aziz built a Tent»; har o'yinchining
      hissasi (`PlayerInfo.built/demolished`) statistikada, reconnect'da saqlanadi.
- [x] **Rollar (egalik)**: birinchi kirgan o'yinchi — **egasi (Owner)**, qolganlar
      mehmon (Guest). ~~Egasi mehmonlar huquqini belgilaydi (`GuestPermission`:
      ViewOnly / Build / Full)~~ — **olib tashlandi (2026-07-16)**: darajali
      mehmon-huquqi tizimi butunlay bekor qilindi, mehmonlar endi doim to'liq
      vakolatga ega (egasi bilan bir xil), faqat mehmonni **chiqarib
      yuborish (kick)** egaga xos bo'lib qoladi. Server har buyruqni
      `GameState::can_issue` orqali tekshiradi (yagona haqiqat manbai).
      Egasi yo'q olam to'liq co-op. Taklif tizimining poydevori.
- [x] **Reconnect**: sessiya tokeni (`Hello.token` / `Welcome.token`) — server
      ulanish-id'ni o'yinchi-id'dan ajratadi, uzilgan o'yinchining `PlayerInfo`'sini
      token bo'yicha saqlab, xuddi shu o'yinchi (id + stats) sifatida qaytaradi.
      Mijozda avtomatik qayta-ulanish (Join rejimi).
- [x] **Delta-snapshot**: 2026-07-10. `tiles`dagi mavjud tashlab-ketish andozasi
      `events`/`chat`/`pings`/`missions`/`techs`ga ham tarqatildi (`protocol::Included`
      bayroqlari) — bular ko'pincha o'zgarmay qoladi. `buildings`/`survivors`/`stock`
      atayin tegilmadi: sim.rs'da har tikda uzluksiz o'zgaradi (progress/hunger/decay),
      shu sabab "o'zgarganda yubor" ulardan foyda bermas edi. Asosiy tejov —
      **siqish**: butun bincode freym `miniz_oxide` deflate bilan (TCP va WS
      ikkalasida ham, `protocol::encode`/`decode`).
- [x] **Interpolatsiya**: allaqachon bor edi, faqat roadmap belgilanmagan —
      kursorlar `sync_player_cursors`da eksponensial lerp bilan, aholi esa
      `animate_survivors`da tezlik-asosli yurish bilan, ikkalasi ham snapshot
      kelish tezligidan mustaqil, har frame silliqlanadi.
- [x] **Mustahkamlik**: har-ulanish buyruq rate-limit (Cmd 30/s, Chat 4/s, Ping 6/s),
      frame limiti (mavjud), protokol fuzz testi (5000 random frame → panic yo'q).

### Natija mezonlari

- Snapshot trafigi mijoz boshiga ≤ 1 KB/s (o'lchov e2e testda).
- Uzilish → qayta ulanish → xuddi shu o'yinchi davom etadi (test).
- Har buyruq egasi bilan voqealar lentasida ko'rinadi.
- Mehmon taqiqlangan amalni bajara olmaydi (e2e test).

---

## V0.3 — Missiyalar va Tunnel (shaxsiy olam progressiyasi)

**Maqsad:** shaxsiy olamda qiladigan ish ko'p bo'lsin; progressiya o'yinchini
Tunnelgacha yetaklasin.

### Vazifalar

- [x] **Missiya tizimi**: deterministik quest'lar (chodir qur, aholi, arra zavodi,
      ko'mir zaxirasi, kun omon qol) + resurs mukofotlari; progress snapshot ichida
      (`GameState.missions`), simda deterministik baholanadi. Client'da missiya paneli.
- [x] **G'alaba sharti qayta ishlandi**: Tunnel bitgani = **graduatsiya g'alabasi**
      (Global Olamga chiqish) — «12-kun» g'alaba yonida qo'shimcha yo'l. `graduated`
      bayrog'i simda faqat Tunnel bitgan tarmoqda o'rnatiladi; game-over ekrani ikki
      g'alabani alohida ko'rsatadi («THE TUNNEL IS OPEN» vs «VICTORY»).
      To'liq endless rejim (kun-g'alabani olib tashlash) keyingi qadam.
- [x] **TUNNEL**: ko'p bosqichli megaloyiha — barcha missiyalar bitgach ochiladi,
      `InvestTunnel` buyrug'i bilan bosqichma-bosqich qaziladi (3 bosqich), bitgach
      graduatsiya g'alabasi (Global Olamga chiqish signali). Client'da Tunnel paneli.
      Keyingisi: haqiqiy hub'ga o'tish (V0.5).
- [x] **Yangi binolar** (4 → 8): Issiqxona (Greenhouse — yuqori-output oziq),
      Kasalxona (Hospital — HP tiklash), Oshxona (Kitchen — oziq tejash),
      Ombor (Warehouse — 2026-07-10, staffed bo'lsa qurilish yog'och narxi
      `WAREHOUSE_BUILD_DISCOUNT` (20%) arzonroq — hozirgi iqtisodiyotda haqiqiy
      zaxira-sig'imi tushunchasi yo'qligi sababli shu variant tanlandi).
- [x] **Texnologiya daraxti**: 5 texnologiya (Izolyatsiya, Samarali pech, Asboblar,
      Ratsion, Tibbiyot) — resurs evaziga ochiladi (`Research` buyrug'i), effektlar
      simda qo'llanadi. Client'da modal panel (R bilan ochiladi).
- [x] **Voqealar tizimi**: **kasallik** (HP kamayadi, kasalxona yumshatadi),
      **qor bo'roni** (kuchli sovuq), va **qochoqlar karvoni — tanlov** (qabul
      qil/rad et: oziq evaziga aholi). Alohida event-RNG (asosiy sim RNG'ga
      tegmaydi) + grace-period (3-kundan). Client'da karvon popup + status indikatorlar.
- [~] **Balans regression testlari**: 29 missiya/tunnel/bino/texnologiya/voqea sim-testi.

### Natija mezonlari

- Yangi o'yinchi birinchi missiyalar orqali yordamisiz o'rganadi (playtest — hali qilinmagan).
- Tunnelgacha kamida 2–3 soat mazmunli kontent bor.
- ✅ Sim testlar 18 → 101 (29 tasi missiya/tunnel/bino/texnologiya/voqea uchun); endless
  rejim uzoq muddatli barqarorlik testi hali alohida yozilmagan.

---

## V0.4 — Akkauntlar va doimiy shaxsiy olamlar

**Maqsad:** shaxsiy olam serverda yashaydi — istalgan qurilmadan kirsa bo'ladi,
hech qachon yo'qolmaydi.

**Holat: ✅ to'liq bajarildi (2026-07-12).** Client-ichidan ro'yxatdan o'tish
(menyuda Register rejimi, Telegram bot ham ishlayveradi), cross-device
`tests/cross_device_e2e.rs`da isbotlangan (server restart bilan birga), 50+
parallel shaxsiy olam yuk testi o'tdi.

### Vazifalar

- [x] **Akkauntlar**: rejadagidek Tunnel-bog'liq emas, Telegram bot
      (`bot/register_bot.py`) orqali istalgan vaqt ro'yxatdan o'tiladi — bcrypt bilan
      SQLite'da (`/var/lib/frozen-city-accounts/accounts.db`) saqlanadi, server
      `ClientMsg::Login`/`AuthFailed` bilan tekshiradi, sessiya V0.2 reconnect
      tokeni ustiga quriladi (`crates/fc-net/src/accounts.rs`). ✅ **Ro'yxatdan
      o'tish endi client ichidan ham** (2026-07-12): menyuda Register rejimi
      (`ClientMsg::Register`, server `register_account` — bot sxemasi bilan bir
      xil jadval, sintetik manfiy telegram_id, jarayon-keng 5/min rate-limit),
      muvaffaqiyatda darhol o'z shaxsiy olamiga kiradi.
- [x] **Server tomonida persistensiya, har akkaunt uchun alohida** (2026-07-10):
      `src/net/world_manager.rs` — login bo'lganda `account_id` bo'yicha alohida
      sim_loop thread lazy-spawn qilinadi, o'z faylida saqlanadi
      (`/var/lib/frozen-city/accounts/{id}.bin`, `persist.rs`ning save_at/load_at'i
      qayta ishlatilgan), 300s faolsizlikdan keyin avto-evict+save, 200 olamgacha
      cap. Guest (`Hello`, akkauntsiz) hamon umumiy olamga kiradi, o'zgarmagan.
      SQLite/Postgres emas, oddiy fayl-per-akkaunt — hozirgi masshtabda yetarli,
      kelajakda kerak bo'lsa almashtiriladi.
- [x] **Cross-device**: akkaunt bilan kirgan har qanday klient (brauzer/desktop)
      `account_id` orqali xuddi shu olamga marshrutlanadi
      (`WorldManager::join_account`). ✅ **Aniq isbotlangan (2026-07-12)**:
      `tests/cross_device_e2e.rs` — bitta akkaunt, ikkita ketma-ket mustaqil
      ulanish ("ikki qurilma") bir xil shahar/binolarni ko'radi, jonli umumiy
      olamda qurishda davom etadi, va to'liq server restart'idan keyin ham
      uchinchi ulanish hammasini joyida topadi. ~~Nuance: region-mahalliylik~~ —
      **butunlay bekor bo'ldi (2026-07-16)**: ko'p-region infratuzilmasining
      o'zi olib tashlandi (yuqoriga qarang), shu sabab bu masala endi mavjud
      emas — bitta jarayon, akkauntlar va markaziy olam har doim o'sha yerda.

### Natija mezonlari

- [x] Server restart → akkaunt olami tiklanadi (avtomatlashgan test,
      `tests/account_world_e2e.rs`) + production'da ham qo'lda tasdiqlangan.
- [x] Brauzerda boshlagan o'yinchi desktopdan o'sha shahriga kiradi —
      `tests/cross_device_e2e.rs` (2026-07-12), restart bilan birga.
- [x] 50+ shaxsiy olam bitta serverda parallel — `tests/scale_e2e.rs`
      (2026-07-12): 50/50 olam mustaqil tick, join p50=800ms p99=956ms,
      RSS 152MB (production hostida o'lchangan).

---

## V0.5 — Global Olam (hub)

**Maqsad:** butun dunyo o'yinchilari uchrashadigan bitta doimiy makon.

**Holat: ✅ bajarildi (2026-07-12).** Markaziy olam + yengil avatar-rejim
(har o'yinchi kursor tayl'i tomon yuradigan nomli figura — protokol
o'zgarishisiz, mavjud kursor sinxronidan), do'stlar ro'yxati + social panel,
yaqin-atrof chat pufakchalari, vitrina (Showcase), akkaunt-asosli bino
egaligi va hissa daftari. 100-klient yuk testi va <5s o'tish o'lchab
tasdiqlangan. To'liq gateway-shardli interest management ataylab V1.0+ ga
qoldirildi (quyida).

### Vazifalar

- [x] **Hub-rejim**: bitta doimiy markaziy olam (`sim::new_game_central`,
      `GameState::central`): ochlik/o'lim/voqealar/g'alaba-mag'lubiyat yo'q,
      aholi faqat Tunnel orqali keladi (`extract_migrants`/`inject_migrants`,
      akkauntga 5 tagacha, qaytib kirish nusxalamaydi — cap'gacha to'ldiradi),
      **har kim faqat o'z ko'chmanchilarini boshqaradi** (`can_issue`ning
      central-tarmog'i, roster ham faqat o'znikini ko'rsatadi), Owner roli
      yo'q. ✅ **Yengil avatar-rejim (2026-07-12)**: markaziy olamda har ulangan
      o'yinchi nom yorlig'li low-poly figura sifatida ko'rinadi va kursor
      tayl'i tomon yuradi (`render.rs::sync_avatars`/`animate_avatars` —
      protokol o'zgarishisiz, mavjud kursor sinxroni ustiga); chat
      pufakchalari avatar tepasida suzadi. ✅ **Akkaunt-asosli bino egaligi**
      (`Building.owner_account` — faqat egasi buza oladi) va **hissa daftari**
      (`central_ledger`: har akkauntning ishlab chiqarish/sarf hissasi,
      deterministik) — 2026-07-12, FCWORLD3 saqlov-migratsiyasi bilan.
- [x] **Global va yaqin-atrof chat, do'stlar ro'yxati** (2026-07-12): global
      chat mavjud edi; endi `/l` prefiksli **yaqin-atrof chat** (`ChatLocal` →
      12 tayl radiusdagi o'yinchilarga `Bubble`, GameState'da saqlanmaydi),
      pufakchalar yuboruvchining avatari/kursori tepasida 7s suzib so'nadi;
      **do'stlar ro'yxati** server-tomonda SQLite'da (`friends` jadvali,
      qo'shish/o'chirish/ro'yxat, `Social` snapshot'i), klientda social panel
      (`F` tugmasi yoki HUD "Friends" tugmasi — mobil uchun).
- [x] **Tunnel o'tish oqimi**: graduatsiya ekranidagi "Enter the Global World"
      tugmasi va HUD'dagi "Global World"/"My City" almashtirgichi
      (`PendingSwitch`); uzilishda reconnect `EnterCentral`ni qayta jo'natadi.
      ✅ O'tish overlay xabari qo'shildi (2026-07-12, `TransitionMsg`:
      "Entering the Global World…" fade-in/out). O'lchangan o'tish vaqti:
      0.78–0.89s (mezon <5s).
- [ ] **Interest management** — *ataylab V1.0+ ga qoldirildi*: 100-klient yuk
      testi hozirgi to'liq-snapshot + deflate yondashuvida MUAMMOSIZ o'tdi
      (0 uzilish), ya'ni bu masshtabda zona-filtrlash hali shart emas. Yaqin-
      atrof chat allaqachon 12-tayl radius bilan ishlaydi. Gateway + shardlash
      1000+ concurrent'da qaytib ko'riladi.
- [x] **Hub mashg'ulotlari (v1)** (2026-07-12): **vitrina (Showcase)** —
      social panelda har do'stning shahar statistikasi (kun/aholi/bino/Tunnel
      belgisi), `RefreshShowcase` → saqlov faylidan o'qiladi (5s cooldown,
      faqat do'stlarga — maxfiylik; o'qish tick thread'idan tashqarida,
      alohida thread'da). E'lonlar taxtasi/savdo — keyinroq, dizayn ochiq.

**Muhim texnik asos:** saqlash formati versiyalangan — hozir `FCWORLD3`
(2026-07-12: `Building.owner_account` + `central_ledger` uchun); `FCWORLD2`
va magic'siz (V1) fayllar `fc-net/src/legacy.rs`dagi muzlatilgan V1/V2
ko'zgu-strukturalar orqali V1→V2→V3 zanjirida migratsiya qilinadi (uchala
haqiqiy production saqlov nusxasida tekshirilgan). `GameState`ga maydon
qo'shishdan OLDIN har doim: yangi versiya magic + legacy zanjiriga yangi
bo'g'in, va deploy'dan oldin haqiqiy production saqlovlar nusxasini
`examples/checksave.rs` bilan tekshirish.

### Natija mezonlari

- [x] 100+ concurrent klient bitta hub'da — `tests/scale_e2e.rs` (2026-07-12):
      100/100 ulandi, kuzatuv oynasida 0 uzilish, join p50=898ms.
- [x] Shaxsiy olam ↔ hub o'tish < 5 soniya — o'lchandi: 0.78–0.89s
      (`tests/full_cycle_e2e.rs`).
- [x] Do'st qo'shish ikkala tomonda ham saqlanadi — SQLite `friends` jadvali,
      server restart'dan omon qoladi (`tests/social_server_tests.rs` +
      `accounts.rs` unit testlari).

---

## V0.6 — Taklif tizimi va mehmon co-op

**Maqsad:** vizyonning yakuniy halqasi — hub'dagi do'stni o'z olamingga olib kirish.

**Holat: ✅ to'liq bajarildi (2026-07-12).**

### Vazifalar

- [x] **Taklif**: hub'da do'stga taklif yuborish (`Invite`, social paneldagi
      tugma) → `Invited` bildirishnomasi + qabul qilsa `VisitFriend` bilan
      egasining shaxsiy olamiga ulanadi (`InviteBook`, 15 min TTL). Taklif
      endi nishonning **shaxsiy olamiga ham** yetib boradi
      (`WorldManager::deliver_to_account`) — do'st hub'da turishi shart emas;
      hub'da yetkazilganda shaxsiy olamga TAKRORLANMAYDI (ikki qurilmali
      akkaunt bitta taklifni ikki marta ko'rmaydi).
- [x] **Mehmon huquqlari** (V0.2 rollar ustiga): olam egaligi endi **akkauntga
      mahkamlangan** (`ServerConfig::owner_account` — tashrifchi birinchi
      kirib ham Owner bo'lolmaydi); ~~egasi `GuestPermission`ni belgilaydi
      (ViewOnly/Build/Full)~~ — **2026-07-16: bekor qilindi**, egasi endi kick
      qiladi xolos, tashrifchiga ham xuddi shunday amal qiladi
      (`tests/visit_e2e.rs`da tasdiqlangan).
- [x] **Egasiz kirish siyosati**: `allow_offline_guests` sozlamasi (standart:
      YO'Q) — akkaunt DB'dagi server-egalik `visit_policy` jadvali,
      `SetVisitPolicy`/`VisitPolicy` protokol jufti, social paneldagi toggle.
      Yoqilgan bo'lsa taklif qilingan mehmon egasi oflayn olamga ham kiradi
      (olam lazy-spawn bo'ladi), egalik baribir egasida qoladi.
- [x] **Onboarding'siz mehmon**: Tunnel ochmagan do'st ham taklifga kira oladi
      — `visit_friend` yo'lida graduatsiya talabi yo'q, taklif shaxsiy olamga
      yetib borgani uchun hub'ga kira olmasligi to'siq emas
      (`tests/visit_e2e.rs` (g)).

### Natija mezonlari

- [x] To'liq tsikl e2e testi — `tests/full_cycle_e2e.rs` (2026-07-12): jonli
      InvestTunnel graduatsiya → hub → taklif → mehmon tashrifi → mehmon
      binoni quradi → voqealar lentasida mehmon nomi bilan attributsiya.
      Eslatma: yangi graduatsiya bo'lgan olam `WORLD_RESET_AFTER` (45s)
      qayta-boshlashgacha buyruq qabul qilmaydi (Won fazasi) — test buni
      kutadi; UX sayqali V1.0 ro'yxatida.
- [x] Kick va huquq cheklovlari testda tasdiqlanadi — `tests/visit_e2e.rs`:
      taklif­siz rad, egasi-oflayn (standart) rad, Guest roli, ViewOnly
      no-op, Build + attributsiya, kick → ulanish uziladi.

---

## V0.7 — Aholi boshqaruvi (harakat, yetakchi, kasb, XP, morale)

**Maqsad:** aholini anonim ish-kuchi hisoblagichidan boshqariladigan, o'ziga
xos personajlarga aylantirish.

**Holat: ✅ to'liq bajarildi (2026-07-12).** Dastlabki rejada bu bosqich yo'q
edi (V0.6 → V1.0); alohida bosqich sifatida keyin qo'shildi.

### Vazifalar

- [x] **Harakat buyrug'i** (`ClientMsg::MoveSurvivor`): egasi aholini tanlab
      xaritadagi istalgan katakka yuboradi; buyruq aholini ishdan bo'shatadi;
      pozitsiyalar (`Survivor.x/y`) server-avtoritativ, klientda silliqlangan
      animatsiya, touch'da ham ishlaydi (`tests/movement_tests.rs`,
      `tests/survivor_control_e2e.rs`).
- [x] **Yetakchi** (`ClientMsg::SetLeader`, owner-only): tirik yetakchi butun
      shaharga +8% ishlab chiqarish (`LEADER_PRODUCTION_BONUS`); o'lsa bir
      o'yin-kun motam −15% (`MOURNING_PRODUCTION_PENALTY`); voqealarga javob
      (`RespondEvent`) endi tirik yetakchini talab qiladi
      (`tests/leader_tests.rs`). Markaziy olamda yetakchi yo'q
      (`GameState::leader = None`).
- [x] **Kasblar**: 6 kasb (o'tinchi/konchi/ovchi/fermer/shifokor/oshpaz),
      aholi id'sidan deterministik; kasbga mos binoda +25%
      (`PROFESSION_MATCH_BONUS`) (`tests/profession_tests.rs`).
- [x] **XP/darajalar**: biriktirilgan bino turida ishlaganda XP yig'iladi,
      3 darajagacha (+5%/daraja); boshqa bino turiga o'tkazilsa XP nolga
      qaytadi, ishdan bo'shatish esa XP'ni saqlaydi (`tests/xp_tests.rs`).
- [x] **Koloniya morale (0–100)**: o'lim/ochlik/bo'ron pasaytiradi,
      oshxona/kasalxona/yetakchi ko'taradi, baseline tomon drift; ishlab
      chiqarishga ko'paytma bo'lib kiradi (`GameState::morale_multiplier`,
      `tests/morale_tests.rs`); HUD'da ko'rsatkich.
- [x] **Aholi tafsilot kartasi** roster panelida: kasb, daraja, holat —
      har doim ochiladigan kichik panel (`src/client/roster.rs`).
- [x] **Saqlov FCWORLD4**: V3→V4 migratsiya `legacy.rs` zanjirida (V1→V2→V3→V4).
      Eski binary V4'ni o'qiy olmaydi — rollback saqlovlarni ham qaytarishni
      talab qiladi.

### Natija mezonlari

- [x] To'liq release test to'plami yashil (6 yangi test-fayl bilan),
      `clippy --all-targets` toza, wasm check toza, native smoke exit 0
      (2026-07-12 ~14:10 UTC'da to'rttalasi tasdiqlangan).

---

## V1.0 — Sayqal va tarqatish

**Maqsad:** keng auditoriyaga chiqishga tayyor mahsulot.

### Vazifalar

- [~] **Ovoz**: ✅ protsedural (assetsiz) effektlar — qurish "thunk", voqea chime,
      qor bo'roni shamoli (blizzard'da kuchayadi). Qoldi: fon musiqasi, ko'proq SFX.
- [~] **Vizual sayqal**: ✅ bino qurilish animatsiyasi (o'sish), qor bo'roni effekti
      (qalin qor + whiteout osmon/tuman + sovuq tint), tunda aurora. ✅ aholi
      personaj-modellari (2026-07-14): protsedural (assetsiz) qishki
      ko'rinish — kasbiga mos plash rangi + bosh kiyimi (peshtaxta/kaska/
      to'quv shlyapa) + qurol/anjom (bolta, kirka, miltiq, savat, xoch
      nishoni, cho'mich) — har biri ish joyining o'z materialini qayta
      ishlatadi (masalan o'tinchi boltasi arra zavodi tig'idan). Yurganda
      oyoqlar protsedural tebranadi (`animate_survivor_legs`); yuk
      ko'targanda orqada taxta ko'rinadi. Qoldi: tutun sayqal, o'tishlar.
- [~] **Unumdorlik va deploy**: ✅ WebGPU, gzip serving + cache header, umumiy bino
      materiallari (draw-call kamaytirish), release `strip`, Cargo.lock tracked,
      build-web.sh wasm o'lchamini ko'rsatadi, ✅ wasm-opt (binaryen), ✅ delta-snapshot
      + freym siqish (V0.2'da). Qoldi: bevy feature-trim.
- [~] **Lokalizatsiya** (2026-07-12): ✅ butun KLIENT UI uch tilda (uz/en/ru) —
      i18n qatlami tashqi crate'siz (`i18n.rs` + soha kataloglari `i18n_menu/
      i18n_hud/i18n_panels/i18n_names.rs`, har matn exhaustive-match funksiya:
      tarjima tushib qolsa kompilyatsiya xatosi); kirill uchun DejaVu Sans Mono
      binary'ga embed qilinib standart shrift almashtirilgan; til tanlash:
      menyu / `?lang=` / `--lang`, tanlov saqlanadi (web localStorage, native
      `~/.frozen-city/settings.kv`). QOLDI: server-tomonda yaratiladigan matnlar
      (voqealar lentasi `GameEvent.text`, auth/social javoblari) — protokolga
      message-key enum talab qiladi (append-only, saqlov ta'siri bilan) —
      alohida keyingi qadam; accessibility (rang-ko'r palitra).
- [x] **Sozlamalar menyusi** (2026-07-12): til / grafika darajasi
      (Avto-Past-O'rta-Yuqori, keyingi ishga tushirishda kuchga kiradi) / ovoz
      on-off — menyuning SOZLAMALAR bo'limida, barchasi saqlanadi.
- [x] **UI dizayn-tizimi va moslashuvchan layout** (2026-07-12, rejadan
      tashqari qo'shildi): `theme.rs` — yagona palitra ("muz" vizual tili),
      tipografika/masofa shkalasi, `FormFactor` (Mobile<720/Tablet<1160/
      Desktop) va umumiy vidjetlar (panel/scrim/card/button). Barcha UI
      qayta ishlangan: mobilda modallar pastki-varaq (bottom sheet), qurish
      paneli gorizontal scroll + ≥46px barmoq nishonlari, HUD ikki qatorli
      kompakt; UiScale endi 0.8 dan pastga tushmaydi. Qayta ishlashda 3 ta
      yashirin ECS query-konflikt smoke-gate'da ushlanib tuzatildi.
- [ ] **CI/CD**: GitHub Actions — test + Windows/Linux/macOS/wasm artefaktlari.
- [~] **PWA**: ✅ bosh ekranga o'rnatish (manifest + service worker + ikonkalar,
      network-first strategiya). Qoldi: Android, keyin iOS (native qadoqlash).
- [ ] **itch.io sahifasi**: skrinshotlar, gif-treyler, web-versiya embed.

### Natija mezonlari

- Bitta teg → 5 platforma artefakti CI'dan chiqadi.
- O'rta darajali telefonda 30+ FPS.
- Uch tilda to'liq UI.

---

## Doimiy tamoyillar (har fazada)

1. **`src/game/` Bevy'siz qoladi** — sim sof funksiya bo'lib testlanadi va WASM'da ishlaydi.
2. **Determinizm buzilmaydi** — har yangi mexanika `--seed` bilan takrorlanadi.
3. **Yakka o'yin = xuddi shu server** — shaxsiy olam ham, hub ham bitta protokolda.
4. **Shaxsiy olam sim'i arzon qolsin** — minglab olam parallel ishlashi kerak;
   faqat hub umumiy, qolgan hamma narsa izolyatsiyalangan.
5. **Har feature test bilan keladi** — sim-invariant yoki e2e.
6. **Bitta port printsipi** — TCP + WS + HTTP birgalikda; deploy sodda qolsin.
7. **`src/client/`ga tegadigan har o'zgarish smoke-test bilan tasdiqlanadi** —
   `cargo test` faqat `src/game`/`src/net` sof mantiqni tekshiradi, Bevy client
   (ECS/render) hech qachon haqiqatda ishga tushirilmaydi. Runtime-only xatolar
   (masalan ECS query-conflict — ikkita so'rov bir komponentga mos kelmagan
   holda murojaat qilishi, faqat schedule ishga tushganda paydo bo'ladi)
   testlardan sizib o'tadi va o'yin sahifasi ochilgan zahoti qulaydi (2026-07-10
   incident: roster.rs'dagi shunday xato production'ga tushib ketgan edi).
   Shu sabab `deploy.sh`ga majburiy bosqich qo'shilgan: `xvfb-run -a timeout 150
   target/release/frozen_city --smoke`, muvaffaqiyatsiz (panik yoki
   non-zero exit) bo'lsa deploy to'xtaydi, web build va servis almashtirish
   bosqichigacha yetib bormaydi. Qo'lda deploy qilganda ham shu buyruqni
   ishga tushirish shart.

## Tavsiya etilgan tartib va taxminiy hajm

| Faza | Holat | Nega shu tartibda |
|---|---|---|
| V0.2 Tarmoq poydevori | ✅ bajarildi | Attributsiya, rollar, reconnect — hamma ijtimoiy narsaning asosi |
| V0.3 Missiyalar + Tunnel | ✅ bajarildi | Shaxsiy olam kontenti — o'yinchini hub'gacha yetaklaydi |
| V0.4 Akkauntlar + doimiy olamlar | ✅ bajarildi (2026-07-12) | Taklif va hub uchun identity + persistensiya shart |
| V0.5 Global Olam (hub) | ✅ bajarildi (2026-07-12) — interest management V1.0+ ga qoldirildi | Eng katta yangi ish edi: yengil avatar rejimi + masshtab |
| V0.6 Taklif + mehmon co-op | ✅ bajarildi (2026-07-12) | Vizyon halqasi yopildi |
| V0.7 Aholi boshqaruvi | ✅ bajarildi (2026-07-12) | Aholi shaxsiylashuvi: yetakchi/kasb/XP/morale — retention chuqurligi |
| V1.0 Sayqal + tarqatish | qisman (yuqoriga qarang) | Keng auditoriyadan oldin oxirgi qatlam |

**Keyingi konkret qadamlar (V1.0 yo'lida, 2026-07-12 holatiga):**

1. ~~**Graduatsiya UX sayqali**~~ ✅ bajarildi (2026-07-12): server endi
   game-over (Won/Lost) davomida `ServerMsg::ResetCountdown` yuboradi
   (soniya qiymati o'zgarganda bitta), overlay "A new expedition arrives
   in N s." qatorini ko'rsatadi — 45s jim muzlash endi ko'rinadigan sanoq
   (`tests/social_server_tests.rs`da e2e tasdiqlangan).
2. ~~**`GameState::tile()` delta-snapshot ehtiyotkorligi**~~ ✅ bajarildi
   (2026-07-12): `tile()` endi `Option<&Tile>` qaytaradi (`tiles.get`) —
   bo'sh/delta holatda panik o'rniga `None`; chaqiruvchilar moslandi.
3. ~~**Lokalizatsiya (uz/en/ru)** va sozlamalar menyusi~~ ✅ bajarildi
   (2026-07-12): klient UI to'liq uch tilda + sozlamalar (til/grafika/ovoz,
   saqlanadi) + dizayn-tizim/responsive qayta ishlash — batafsili "V1.0"
   bo'limida. Qoldiq: server matnlari (voqealar lentasi) protokol
   message-key'lari orqali — quyidagi 4-band bilan birga rejalashtirilsin.
4. **Server matnlarini lokalizatsiya qilish** — `GameEvent.text` o'rniga
   message-key enum + parametrlar (protokol append-only, saqlovda eski
   String'lar bilan moslik kerak); shundan keyin voqealar lentasi ham uch
   tilda bo'ladi.
5. **CI/CD (GitHub Actions)** — test + artefaktlar; deploy hozir serverda
   `deploy.sh` orqali.
6. **Interest management / gateway shardlash** — 1000+ concurrent uchun;
   100 klientgacha hozirgi arxitektura o'lchab tasdiqlangan.
7. **Markaziy olam savdo/e'lonlar taxtasi** — hissa daftari (`central_ledger`)
   endi bor, uning ustiga quriladi.

Shu kunning o'zida yopilgan ikki mayda qoldiq: `Invited` endi ikki
ulanishli akkauntga faqat bir marta boradi (markazda yetkazilsa shaxsiy
olamga takrorlanmaydi), va `RefreshShowcase`ning saqlov-fayl o'qishlari
tick thread'idan alohida throwaway thread'ga ko'chirildi (katta do'stlar
ro'yxati endi olam tick'ini to'xtatmaydi).
