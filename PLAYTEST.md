# FROZEN CITY — Playtest rejasi

> Maqsad: **ko'r uchishni tugatish.** Shu paytgacha loyihaning har «qabul
> mezoni» avtomatik test bilan bajarilgan — 50 parallel olam, 100 klient bir
> hub'da. Lekin bularning hammasi **bot**, odam emas. Yagona bajarilmagan
> mezon — insoniy: *o'yin qiziqarlimi va odam qaytadimi?* Bu hujjat shuni
> o'lchash va tuzatish tsiklini belgilaydi.

Muhandislik poydevori mustahkam. Endi eng qiyin, eng noqulay ishni qilamiz:
**odamlarni o'ynatib, ma'lumot yig'amiz.**

---

## 1. Telemetriya — endi nima o'lchanadi

Server har ulanishga ikki voqea yozadi (`crates/fc-net/src/telemetry.rs`):

- **`session_start`** — join'da (kim, qaysi olam, reconnect'mi).
- **`session_end`** — ketishda, **progress snapshot** bilan: o'yin-kun, faza,
  graduated, binolar soni, aholi, missiya (bajarilgan/jami), tunnel bosqichi,
  zaxira. Sessiya uzunligi ham shu yerda.

Shu ikki voqeadan quyidagilar chiqadi:

| Signal | Nima aytadi |
|---|---|
| **DAU / bir vaqtdagi cho'qqi** | Umuman kim o'ynayapti, qachon |
| **Sessiya uzunligi** (p50/p90) | Bitta o'tirish qancha davom etadi |
| **Tashlab-ketish kuni** (drop-off day) | ⭐ *Qaysi o'yin-kunida chiqib ketishadi* — qiziqish qayerda tugaydi |
| **Vizyon funneli** | Nechta akkaunt Tunnelga yetadi → graduatsiya → Global Olam |

### Yoqish

Production systemd unit'iga (`frozen-city`) qo'shing:

```ini
Environment=FC_TELEMETRY_PATH=/var/lib/frozen-city/telemetry.jsonl
```

O'rnatilmasa telemetriya **butunlay no-op** — test/singleplayer/`--host` hech
narsa yozmaydi, disk tegilmaydi. (`/var/lib/frozen-city/` allaqachon olam
saqlash katalogi, ruxsatlar tayyor.)

### O'qish

```bash
python3 bot/analyze_telemetry.py                       # standart yo'l
python3 bot/analyze_telemetry.py /path/to/file.jsonl --since 2026-07-14
```

Matn hisobot beradi: kunlik faollik, sessiya uzunligi persentillari,
**tashlab-ketish-kun gistogrammasi** va funnel. Stdlib-only (server'da bot
uchun Python allaqachon bor).

> **Maxfiylik:** faqat allaqachon mavjud identifikator yoziladi (akkaunt id,
> ekran-nomi). Guest'lar akkauntsiz — sessiya sifatida sanaladi, shaxs
> kuzatilmaydi. Yangi PII yo'q.

---

## 2. Playtest protokoli (5–10 real odam)

Botlar emas, **haqiqiy odamlar** kerak. Kichik boshlang — sifat > son.

### Kim
5–10 kishi: 2–3 tasi o'yinni umuman bilmaydigan (eng qimmatli — ular
qayerda adashishini ko'rsatadi), qolgani citybuilder o'ynaydigan.

### Qanday
1. **Single-player loopdan boshla.** Global Olam/taklif emas — avval «shahar
   qur, omon qol, Tunnelgacha yet» tsikli qiziqarli ekanini isbotla.
2. **Yoningda kuzat** (yoki ekran-yozuv + ovoz). Yordam **berma** — qayerda
   qotib qolishini ko'r.
3. Har seansdan keyin **3 ta savol** (ko'p emas):
   - «Nima qilishing kerakligini qachon tushunmay qolding?»
   - «Qaysi daqiqada zerikting yoki chiqib ketging keldi?»
   - «Yana o'ynaysanmi? Nega?»
4. Telemetriyani yoq — kuzatuvingni raqam tasdiqlaydi (yoki rad etadi).

### Nimaga qara (kuzatuvda)
- Birinchi 60 soniyada nima qiladi? Adashadimi?
- Missiyalarni **o'qiydimi**, yoki e'tibormay quradi?
- Qaysi o'yin-kunida «bo'ldi, tushundim» deb zeriktiradi?
- Pech/harorat/ochlik mexanikasini yordam‑siz tushunadimi?

---

## 3. Ma'lumot qanday qaror chiqaradi

| Topilma | Harakat |
|---|---|
| Drop-off day gistogrammasi 2–3-kunda cho'qqi | Qiziqish shu yerda tugaydi — **yangi feature emas**, o'sha oynani chuqurlashtir (balans, missiya, voqea) |
| Funnel «placed a building»da qulaydi | Onboarding buzuq — birinchi 5 daqiqa ustida ishla |
| Funnel «started the Tunnel»gacha yetмaydi | Tunnelgacha kontent yupqa/zerikarli — oradagi 2–3 soatni to'ldir |
| Sessiya p50 < 10 daqiqa | Loop juda tez tugaydi yoki tez zeriktiradi |
| Akkaunt qaytmaydi (DAU tekis, retention yo'q) | «Qaytish sababi» yo'q — bu Global Olam qurishdan MUHIMROQ |

---

## 4. Intizom — «oldinga qurishни to'xtat»

Keyingi qadam **yangi sistema emas**. Tartib:

1. **Telemetriyani yoq** (bir qatorlik env o'zgarish).
2. **5–10 odamни o'ynat**, kuzat, 3 savolni ber.
3. Bir hafta telemetriyani yig', `analyze_telemetry.py` bilan o'qi.
4. **Drop-off kunini** birinchi tuzat — qolgan hamma narsadan oldin.
5. Faqat single-player retention isbotlangach, Global Olam/taklif
   qatlamiga qayt (u to'g'ri *pirovard* arxitektura, lekin bevaqt qurilgan —
   populyatsiyasiz hub bo'sh).

> **Qoida:** «sistemalar soni ≠ odamlar sevadigan o'yin». Bu tsikl tugamaguncha
> yangi feature qo'shilmaydi.

---

## 5. Keyingi telemetriya qadamlari (v2, hozircha shart emas)

- **Milestone-timing voqealari** — birinchi bino / birinchi missiya / Tunnel
  boshlanishigacha *qancha vaqt* ketdi (hozir funnel «yetdi/yetmadi»ni beradi,
  «qachon»ни emas).
- **Vizual dashboard** — `analyze_telemetry.py` o'rniga HTML/grafik (drop-off
  egri chizig'i ko'zga ko'rinsin).
- **Graceful-shutdown flush** — hozir per-qator flush; qattiq kill'da faqat
  kanaldagi in-flight voqea yo'qoladi (kam). Kerak bo'lsa shutdown'da drain.
