# FROZEN CITY — Yo'l xaritasi (Roadmap)

> Dolzarb rivojlanish rejasi. Dastlabki dizayn-hujjat: [PLAN.md](PLAN.md).
> Yangilangan: 2026-07-10.

## Hozirgi holat (V0.1 MVP + V0.2 + V0.3 — barchasi tayyor)

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
- **Ko'p-region infratuzilmasi**: 3ta mustaqil static olam (asosiy + region2 + region3,
  alohida systemd xizmat va portlarda), brauzerda region tanlash menyusi, PWA
  (manifest+service worker), yuk-test vositasi (`examples/loadtest.rs`).
- **Markaziy olam (V0.5'ning birinchi bosqichi, 2026-07-11)**: Tunnel bitgan akkaunt
  `EnterCentral` bilan bitta doimiy **Global Olamga** o'tadi; o'tishda shaxsiy
  olamidan 5 tagacha aholi **ko'chib o'tadi** (Tunnel orqali, shaxsiy olamdan
  chiqib ketadi) va markaziy olamda **faqat egasi boshqaradigan** ko'chmanchilarga
  aylanadi (`Survivor::owner`, akkaunt bo'yicha). Markaziy olamda ochlik/o'lim/
  g'alaba/mag'lubiyat yo'q — doimiy uchrashuv maydoni. Saqlash formati
  versiyalandi (`FCWORLD2` + V1 migratsiya, `net/legacy.rs`) — eski production
  olamlar buzilmasdan o'qiladi. Akkauntlar va markaziy olam **faqat asosiy
  regionda** (klient login/EnterCentral'ni `/ws`ga majburlaydi; region2/3
  `FC_DISABLE_ACCOUNTS=1` bilan rad etadi) — aks holda har region o'z
  WorldManager'i bilan bitta akkauntning "yagona" olamini regionlararo
  nusxalarga bo'lib yuborar edi (V0.4'dagi ochiq savol shu tarzda yechildi).

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
      mehmon (Guest). Egasi mehmonlar huquqini belgilaydi (`GuestPermission`:
      ViewOnly / Build / Full) va mehmonni **chiqarib yuboradi (kick)**. Server
      har buyruqni `GameState::can_issue` orqali tekshiradi (yagona haqiqat manbai).
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

**Holat:** asosiy qism bajarildi (2026-07-10, `world_manager.rs` — quyida) — login
qilgan akkaunt endi umumiy olam emas, o'zining alohida saqlanadigan olamiga tushadi.
Ochiq qolgani: client-ichidan ro'yxatdan o'tish, cross-device'ni aniq testda
tasdiqlash, va region-mahalliylik nuance'i (pastga qarang).

### Vazifalar

- [~] **Akkauntlar**: rejadagidek Tunnel-bog'liq emas, Telegram bot
      (`bot/register_bot.py`) orqali istalgan vaqt ro'yxatdan o'tiladi — bcrypt bilan
      SQLite'da (`/var/lib/frozen-city-accounts/accounts.db`) saqlanadi, server
      `ClientMsg::Login`/`AuthFailed` bilan tekshiradi, sessiya V0.2 reconnect
      tokeni ustiga quriladi (`src/net/accounts.rs`). Qoldi: ro'yxatdan o'tish
      to'g'ridan-to'g'ri client ichidan (Telegram'siz).
- [x] **Server tomonida persistensiya, har akkaunt uchun alohida** (2026-07-10):
      `src/net/world_manager.rs` — login bo'lganda `account_id` bo'yicha alohida
      sim_loop thread lazy-spawn qilinadi, o'z faylida saqlanadi
      (`/var/lib/frozen-city/accounts/{id}.bin`, `persist.rs`ning save_at/load_at'i
      qayta ishlatilgan), 300s faolsizlikdan keyin avto-evict+save, 200 olamgacha
      cap. Guest (`Hello`, akkauntsiz) hamon umumiy olamga kiradi, o'zgarmagan.
      SQLite/Postgres emas, oddiy fayl-per-akkaunt — hozirgi masshtabda yetarli,
      kelajakda kerak bo'lsa almashtiriladi.
- [~] **Cross-device**: endi ma'noli — akkaunt bilan kirgan har qanday klient
      (brauzer/desktop) `account_id` orqali xuddi shu olamga marshrutlanadi
      (`WorldManager::join_account`). `tests/account_world_e2e.rs` ikki
      AKKAUNT orasidagi izolyatsiya va restart'dan omon qolishni tasdiqlaydi, lekin
      **bitta akkauntning ikkita turli klientdan ketma-ket kirishi** ("brauzerda
      qurib, uzilib, keyin desktopdan o'sha shaharni ko'rish") hali alohida testda
      aniq isbotlanmagan. ~~Nuance: region-mahalliylik~~ — **yechildi (2026-07-11)**:
      akkauntlar (va markaziy olam) faqat asosiy region processida yashaydi;
      klient akkaunt-login/EnterCentral'ni doim `/ws`ga yo'naltiradi, region2/3
      esa `FC_DISABLE_ACCOUNTS=1` bilan akkaunt kirishini rad etadi. Region
      tanlash faqat mehmon co-op uchun qoldi.
- [ ] **Olam menejeri (ko'p-region miqyosida)**: uchta region hamon 3ta **mustaqil,
      qo'lda ishga tushirilgan** static process (alohida systemd xizmat/port,
      brauzerda region tanlash menyusi) — bu qatlamda hali dinamik emas. E'tibor
      bering: per-akkaunt qatlamda esa dinamik menejer (lazy-spawn/idle-evict)
      endi bor — `world_manager.rs` shu naqshni allaqachon namoyish etadi, faqat
      regionlar darajasida emas.

### Natija mezonlari

- [x] Server restart → akkaunt olami tiklanadi (avtomatlashgan test,
      `tests/account_world_e2e.rs`) + production'da ham qo'lda tasdiqlangan.
- [ ] Brauzerda boshlagan o'yinchi desktopdan o'sha shahriga kiradi — marshrutlash
      logikasi buni ta'minlashi kerak, lekin aniq e2e test/qo'lda tekshiruv qoldi.
- [ ] 50+ shaxsiy olam bitta serverda parallel (yuk testi) — `examples/loadtest.rs`
      bor, lekin ko'p-region sig'imini o'lchash uchun, shaxsiy-olam skalasi uchun emas.

---

## V0.5 — Global Olam (hub)

**Maqsad:** butun dunyo o'yinchilari uchrashadigan bitta doimiy makon.

**Holat (2026-07-11):** birinchi bosqich JONLI — markaziy olam mavjud, Tunnel
orqali kiriladi, har akkaunt o'z ko'chmanchilarini olib o'tadi va faqat ularni
boshqaradi. Hozircha "hub" avatar-rejim emas: o'sha shahar-sim xaritasi, lekin
o'lim/g'alabasiz doimiy makon — o'yinchi o'z aholisini binolarga qo'yadi,
quradi, chat qiladi. Avatar/yengil rejim keyingi bosqich.

### Vazifalar

- [~] **Hub-rejim**: ✅ bitta doimiy markaziy olam (`sim::new_game_central`,
      `GameState::central`): ochlik/o'lim/voqealar/g'alaba-mag'lubiyat yo'q,
      aholi faqat Tunnel orqali keladi (`extract_migrants`/`inject_migrants`,
      akkauntga 5 tagacha, qaytib kirish nusxalamaydi — cap'gacha to'ldiradi),
      **har kim faqat o'z ko'chmanchilarini boshqaradi** (`can_issue`ning
      central-tarmog'i, roster ham faqat o'znikini ko'rsatadi), Owner roli
      yo'q (birinchi kirgan ham kick qila olmaydi). Qoldi: avatar bilan yurish
      o'rniga hozircha shahar-sim ko'rinishi — yengil avatar-rejim keyin.
- [ ] **Global va yaqin-atrof chat**, do'stlar ro'yxati (qo'shish/o'chirish).
      (Oddiy umumiy chat markaziy olamda allaqachon ishlaydi — snapshot'dagi
      mavjud chat tizimi; "yaqin-atrof" va do'stlar ro'yxati yo'q hali.)
- [~] **Tunnel o'tish oqimi**: ✅ minimal ishlaydi — graduatsiya ekranidagi
      "Enter the Global World" tugmasi va HUD'dagi "Global World"/"My City"
      almashtirgichi (`PendingSwitch`: bir freym menyu orqali toza qayta
      qurish); uzilishda reconnect `EnterCentral`ni qayta jo'natadi (shaxsiy
      olamga tushib qolmaydi). Qoldi: yuklash ekrani/silliq o'tish animatsiyasi.
- [ ] **Interest management**: mijoz faqat atrofidagi zonani oladi; kerak
      bo'lganda gateway + region serverlar. Zaminiy infratuzilma qisman bor —
      3ta mustaqil region-server (V0.4'da tasvirlangan) — lekin bu hub/avatar
      shardlash emas, qo'lda ochilgan qo'shimcha statik olamlar.
- [ ] **Hub mashg'ulotlari (v1)**: boshqa shaharlarning vitrinasi (statistika,
      «tashrif»), e'lonlar taxtasi; keyinroq savdo/almashuv — dizayn ochiq.

**Muhim texnik asos (2026-07-11):** saqlash formati endi versiyalangan —
`persist.rs` har saqlovni `FCWORLD2` magic bilan yozadi; magic'siz fayl eski
(V1) format deb `net/legacy.rs`dagi muzlatilgan ko'zgu-strukturalar orqali
o'qilib migratsiya qilinadi. `GameState`ga maydon qo'shishdan OLDIN har doim:
yangi versiya magic + legacy zanjiriga yangi bo'g'in, va deploy'dan oldin
haqiqiy production saqlovlar nusxasini `examples/checksave.rs` bilan tekshirish.

### Natija mezonlari

- 100+ concurrent avatar bitta hub'da (sun'iy yuk testi).
- Shaxsiy olam ↔ hub o'tish < 5 soniya.
- Do'st qo'shish ikkala tomonda ham saqlanadi (persistensiya testi).

---

## V0.6 — Taklif tizimi va mehmon co-op

**Maqsad:** vizyonning yakuniy halqasi — hub'dagi do'stni o'z olamingga olib kirish.

### Vazifalar

- [ ] **Taklif**: hub'da do'stga taklif yuborish → qabul qilsa, sening shaxsiy
      olamingga ulanadi.
- [ ] **Mehmon huquqlari** (V0.2 rollar ustiga): egasi belgilaydi — faqat ko'rish /
      qurish mumkin / to'liq sherik. Yomon mehmonni chiqarib yuborish (kick).
- [ ] **Egasiz kirish siyosati**: egasi oflayn bo'lsa mehmonlar kira oladimi —
      sozlama (standart: yo'q).
- [ ] **Onboarding'siz mehmon**: do'st hali Tunnel ochmagan bo'lsa ham taklifga
      kira oladi (mehmonlik progressiyani bermaydi, faqat yordam).

### Natija mezonlari

- To'liq tsikl e2e testi: missiya → Tunnel → hub → taklif → mehmon binoni quradi
  → voqeada «mehmon X qurdi» ko'rinadi.
- Kick va huquq cheklovlari testda tasdiqlanadi.

---

## V1.0 — Sayqal va tarqatish

**Maqsad:** keng auditoriyaga chiqishga tayyor mahsulot.

### Vazifalar

- [~] **Ovoz**: ✅ protsedural (assetsiz) effektlar — qurish "thunk", voqea chime,
      qor bo'roni shamoli (blizzard'da kuchayadi). Qoldi: fon musiqasi, ko'proq SFX.
- [~] **Vizual sayqal**: ✅ bino qurilish animatsiyasi (o'sish), qor bo'roni effekti
      (qalin qor + whiteout osmon/tuman + sovuq tint), tunda aurora. Qoldi: tutun sayqal, o'tishlar.
- [~] **Unumdorlik va deploy**: ✅ WebGPU, gzip serving + cache header, umumiy bino
      materiallari (draw-call kamaytirish), release `strip`, Cargo.lock tracked,
      build-web.sh wasm o'lchamini ko'rsatadi, ✅ wasm-opt (binaryen), ✅ delta-snapshot
      + freym siqish (V0.2'da). Qoldi: bevy feature-trim.
- [ ] **Lokalizatsiya**: uz / en / ru; accessibility (rang-ko'r palitra, shrift).
- [ ] **Sozlamalar menyusi**: grafika darajasi, ovoz, til.
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
| V0.4 Akkauntlar + doimiy olamlar | 🔶 qisman (yuqoriga qarang) | Taklif va hub uchun identity + persistensiya shart |
| V0.5 Global Olam (hub) | 🔶 boshlandi — markaziy olam + Tunnel o'tishi + ko'chmanchi egaligi JONLI | Eng katta yangi ish: avatar rejimi + masshtab |
| V0.6 Taklif + mehmon co-op | boshlanmagan | Vizyon halqasini yopadi; hammasi tayyor bo'lgach arzon |
| V1.0 Sayqal + tarqatish | qisman (yuqoriga qarang) | Keng auditoriyadan oldin oxirgi qatlam |

**Keyingi uchta konkret qadam (V0.4'ning haqiqatda ochiq qolgan qismi):**

1. ✅ ~~Har akkaunt uchun alohida shaxsiy olam~~ — bajarildi, `world_manager.rs`
   (2026-07-10, yuqoriga qarang).
2. ✅ ~~Olam menejeri (minimal)~~ — bajarildi, per-akkaunt lazy-spawn/idle-evict
   `world_manager.rs`da (2026-07-10). Ko'p-region qatlamidagi menejer hamon ochiq
   (yuqoridagi "Olam menejeri (ko'p-region miqyosida)"ga qarang).
3. **Cross-device tasdiqlash** — hali ochiq: bitta akkaunt bilan brauzerdan kirib
   qurish, uzilib, boshqa klient/qurilmadan o'sha login bilan kirib xuddi shu
   shaharni ko'rish — buni tasdiqlovchi aniq e2e test yoki qo'lda (headless
   chromium) tekshiruv yozish kerak. Shu bilan birga: akkaunt ro'yxatdan o'tishni
   Telegram botsiz, client ichidan qilish (V0.4 "Akkauntlar" qoldig'i) ham navbatda.

**V0.5'ning keyingi qadamlari (markaziy olam v1'dan keyin):**

- Markaziy olamda **avatar/yengil rejim** yoki hozirgi ko'chmanchi-shahar
  modelini boyitish (dizayn tanlovi — foydalanuvchi bilan kelishiladi).
- **Do'stlar ro'yxati** va yaqin-atrof chat.
- Markaziy olam **iqtisodiyoti**: umumiy stock hozir "hamma uchun bitta" —
  per-akkaunt hissa/vitrina, savdo keyinroq.
- Markaziy olamda binolarning egaligi hozir sessiya-pid bo'yicha (buzish
  cheklovi uchun) — akkaunt-asosli qilish kerak.
