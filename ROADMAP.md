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
- ✅ 58 test: sim invariantlari + attributsiya/chat/ping/reconnect/rollar e2e + protokol fuzz
- ✅ Mini-xarita (minimap): butun xaritaning burchakdagi ko'rinishi + bosib borish + pinglar

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

**Qolgan ochiq ishlar:** chiqarib yuborilgan mehmon yangi o'yinchi sifatida qayta
kira oladi (ban ro'yxati kerak); egasi butunlay ketsa egalik o'tmaydi (owner-transfer);
chuqurroq: TLS (wss majburiy) va bounded chiquvchi kanal.

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

- [ ] **Missiya tizimi**: deterministik quest'lar («3 chodir qur», «10 kun omon
      qol», «50 ko'mir zaxira qil»), mukofotlar (resurs, yangi bino/texnologiya
      ochilishi). Birinchi missiyalar tutorial vazifasini ham bajaradi.
- [ ] **G'alaba sharti qayta ishlanadi**: «12-kun» o'rniga missiya-progressiya;
      shahar doimiy yashaydi (endless asos), mag'lubiyat baribir mumkin.
- [ ] **Yangi binolar** (4 → 8+): Kasalxona, Oshxona, Issiqxona, Ombor — missiya
      va texnologiyalar orqali ochiladi.
- [ ] **Texnologiya daraxti**: Tadqiqot punkti + 6–10 texnologiya.
- [ ] **Voqealar tizimi**: kasallik, qochoqlar karvoni (tanlov), qor bo'roni.
- [ ] **TUNNEL**: eng katta qurilish loyihasi — ko'p bosqichli (qazish →
      mustahkamlash → ochilish), katta resurs + missiya shartlari. Bitgach
      Global Olamga o'tish ochiladi. Hozircha: mavjud umumiy serverga o'tish.
- [ ] **Balans regression testlari**: standart strategiyalar sim-testda.

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

- [ ] **Ovoz**: effektlar (qurish, pech, shamol, voqea) + fon musiqasi; WASM'da ham.
- [ ] **Vizual sayqal**: qurilish animatsiyasi, tutun, qor bo'roni/aurora.
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
