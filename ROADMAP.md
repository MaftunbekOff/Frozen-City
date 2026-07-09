# FROZEN CITY — Yo'l xaritasi (Roadmap)

> Dolzarb rivojlanish rejasi. Dastlabki dizayn-hujjat: [PLAN.md](PLAN.md).
> Yangilangan: 2026-07-09.

## Hozirgi holat (v0.1 — MVP tayyor)

M0–M7 bosqichlar yakunlangan:

- ✅ Deterministik sim yadrosi (`src/game/`, Bevy'siz) + invariant testlar
- ✅ Server-avtoritativ co-op: TCP + WebSocket + HTTP bitta portda, 1–8 o'yinchi
- ✅ To'liq 3D low-poly protsedural render, kecha-kunduz, issiqlik glow
- ✅ HUD, qurish/bino/pech panellari, menyu, voqealar lentasi, game-over
- ✅ Brauzer (WASM), URL parametrlari, inline singleplayer
- ✅ Mobil touch boshqaruv va grafika sifat darajalari
- ✅ 87 test: sim invariantlari + attributsiya/chat/reconnect/rollar/missiya/bino/texnologiya/voqea + fuzz
- ✅ Mini-xarita (minimap): butun xaritaning burchakdagi ko'rinishi + bosib borish + pinglar
- ✅ V0.3: **missiyalar**, **Tunnel** (graduatsiya), 3 yangi **bino**,
  **texnologiya daraxti** (5 tech), **voqealar tizimi** (kasallik/bo'ron/karvon-tanlov)
- ✅ **Mobil-web unumdorlik** (V1.0 poydevori): **WebGPU** backend (WebGL2 o'rniga —
  Bevy'ning GPU-driven tez yo'li), server **gzip** beradi (wasm 66MB→15MB, 4.4×) +
  cache header, DPR cap + geometriya/yorug'lik kamaytirish, **umumiy bino materiallari**
  (har bino turi bitta material = kamroq draw-call), FPS diagnostika HUD'da
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

**V0.2 (jarayonda):** chat, attributsiya, reconnect, rate-limit va **rollar/egalik**
yakunlandi; qoldi: **delta-snapshot** va **interpolatsiya**.

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
- **Tarmoq (arxitekturaviy):** to'liq **delta-snapshot** (hozir faqat tiles throttle,
  qolgani har tick to'liq) · WS/TCP kadr **siqilishi** · **bounded** chiquvchi/kiruvchi
  navbat (hozir 30s write-timeout + drain-cap qisman himoya).
- **Moderatsiya/egalik:** **ban ro'yxati** (kicked mehmon qaytadi) · **owner-transfer**
  (egasi butunlay ketsa). Eslatma: cloudflared tunnel ortida barcha ulanish bitta IP —
  per-IP cheklov/ban ishlamaydi, akkaunt-identity (V0.4) kerak.
- **Xavfsizlik (transport):** **TLS/wss** origin'da yo'q — hozir cloudflared tunnel
  HTTPS/WSS'ni chekkada beradi (tunnel deploy uchun yetarli), to'g'ridan-to'g'ri ochiqda emas.
- **Web hajmi:** **bevy feature-trim** · release `opt-level="z"` (wasm) · **WebGL2 fallback**
  build (eski brauzerlar; hozir WebGPU-only + boot.js aniqlash xabari).
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
- [ ] **Delta-snapshot**: faqat o'zgargan qism + siqish; ~30 KB/s → ~1 KB/s.
- [ ] **Interpolatsiya**: kursorlar va aholi harakati snapshot orasida silliq.
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
- [ ] **Yangi binolar** (4 → 8+): Kasalxona, Oshxona, Issiqxona, Ombor — missiya
      va texnologiyalar orqali ochiladi.
- [ ] **Texnologiya daraxti**: Tadqiqot punkti + 6–10 texnologiya.
- [ ] **Voqealar tizimi**: kasallik, qochoqlar karvoni (tanlov), qor bo'roni.
- [x] **TUNNEL**: ko'p bosqichli megaloyiha — barcha missiyalar bitgach ochiladi,
      `InvestTunnel` buyrug'i bilan bosqichma-bosqich qaziladi (3 bosqich), bitgach
      graduatsiya g'alabasi (Global Olamga chiqish signali). Client'da Tunnel paneli.
      Keyingisi: haqiqiy hub'ga o'tish (V0.5).
- [~] **Yangi binolar** (4 → 7): ✅ Issiqxona (Greenhouse — yuqori-output oziq),
      Kasalxona (Hospital — HP tiklash), Oshxona (Kitchen — oziq tejash). Qoldi: Ombor.
- [x] **Texnologiya daraxti**: 5 texnologiya (Izolyatsiya, Samarali pech, Asboblar,
      Ratsion, Tibbiyot) — resurs evaziga ochiladi (`Research` buyrug'i), effektlar
      simda qo'llanadi. Client'da modal panel (R bilan ochiladi).
- [x] **Voqealar tizimi**: **kasallik** (HP kamayadi, kasalxona yumshatadi),
      **qor bo'roni** (kuchli sovuq), va **qochoqlar karvoni — tanlov** (qabul
      qil/rad et: oziq evaziga aholi). Alohida event-RNG (asosiy sim RNG'ga
      tegmaydi) + grace-period (3-kundan). Client'da karvon popup + status indikatorlar.
- [~] **Balans regression testlari**: 29 missiya/tunnel/bino/texnologiya/voqea sim-testi.

### Natija mezonlari

- Yangi o'yinchi birinchi missiyalar orqali yordamisiz o'rganadi (playtest).
- Tunnelgacha kamida 2–3 soat mazmunli kontent bor.
- Sim testlar 18 → 35+; endless rejimda 30+ kun barqaror.

---

## V0.4 — Akkauntlar va doimiy shaxsiy olamlar

**Maqsad:** shaxsiy olam serverda yashaydi — istalgan qurilmadan kirsa bo'ladi,
hech qachon yo'qolmaydi.

### Vazifalar

- [ ] **Akkauntlar**: Tunnel ochilganda (yoki xohlaganda) ro'yxatdan o'tish;
      sessiya tokenlari V0.2 reconnect ustiga quriladi.
- [ ] **Server tomonida persistensiya**: shaxsiy olamlar bazada (boshlanishiga
      SQLite, keyin PostgreSQL) — avtomatik saqlanadi, restart'dan omon qoladi.
- [ ] **Cross-device**: desktop / brauzer / telefon — bitta akkaunt, o'sha shahar.
- [ ] **Olam menejeri**: server ko'p shaxsiy olamni parallel yuritadi
      (har biri arzon sim; uxlayotgan olamlar diskda, kirganda uyg'onadi).

### Natija mezonlari

- Server restart → barcha olamlar tiklanadi (avtomatlashgan test).
- Brauzerda boshlagan o'yinchi desktopdan o'sha shahriga kiradi.
- 50+ shaxsiy olam bitta serverda parallel (yuk testi).

---

## V0.5 — Global Olam (hub)

**Maqsad:** butun dunyo o'yinchilari uchrashadigan bitta doimiy makon.

### Vazifalar

- [ ] **Hub-rejim**: avatar bilan yurish (shahar-sim emas — yengil rejim),
      atrofdagi o'yinchilarni ko'rish, ism/ko'rinish.
- [ ] **Global va yaqin-atrof chat**, do'stlar ro'yxati (qo'shish/o'chirish).
- [ ] **Tunnel o'tish oqimi**: shaxsiy olam ↔ hub bitta klient ichida silliq
      almashadi (ulanishni almashtirish, yuklash ekrani).
- [ ] **Interest management**: mijoz faqat atrofidagi zonani oladi; kerak
      bo'lganda gateway + region serverlar.
- [ ] **Hub mashg'ulotlari (v1)**: boshqa shaharlarning vitrinasi (statistika,
      «tashrif»), e'lonlar taxtasi; keyinroq savdo/almashuv — dizayn ochiq.

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
      build-web.sh wasm o'lchamini ko'rsatadi. Qoldi: wasm-opt (binaryen) o'rnatish,
      bevy feature-trim, delta-snapshot tarmoq.
- [ ] **Lokalizatsiya**: uz / en / ru; accessibility (rang-ko'r palitra, shrift).
- [ ] **Sozlamalar menyusi**: grafika darajasi, ovoz, til.
- [ ] **CI/CD**: GitHub Actions — test + Windows/Linux/macOS/wasm artefaktlari.
- [ ] **PWA** (bosh ekranga o'rnatish) + **Android**, keyin iOS.
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

## Tavsiya etilgan tartib va taxminiy hajm

| Faza | Taxminiy hajm | Nega shu tartibda |
|---|---|---|
| V0.2 Tarmoq poydevori | 1–2 hafta | Attributsiya, rollar, reconnect — hamma ijtimoiy narsaning asosi |
| V0.3 Missiyalar + Tunnel | 2–3 hafta | Shaxsiy olam kontenti — o'yinchini hub'gacha yetaklaydi |
| V0.4 Akkauntlar + doimiy olamlar | 2–3 hafta | Taklif va hub uchun identity + persistensiya shart |
| V0.5 Global Olam (hub) | 3–4 hafta | Eng katta yangi ish: avatar rejimi + masshtab |
| V0.6 Taklif + mehmon co-op | 1–2 hafta | Vizyon halqasini yopadi; hammasi tayyor bo'lgach arzon |
| V1.0 Sayqal + tarqatish | 2–3 hafta | Keng auditoriyadan oldin oxirgi qatlam |

**Birinchi uchta konkret qadam (hozirdan boshlasa bo'ladi):**

1. **Attributsiya + chat** — `apply_command`da player id ishlatish, «kim qurdi»
   voqealari va matnli chat: co-op darhol jonlanadi.
2. **Reconnect (sessiya tokeni)** — kelajakdagi akkauntlar shu yerdan boshlanadi.
3. **Missiya tizimining skeleti** — 5–6 ta oddiy quest + mukofot: progressiya
   tuyg'usi paydo bo'ladi, Tunnel sari birinchi qadam.
