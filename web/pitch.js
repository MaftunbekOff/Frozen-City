// Frozen City pitch deck: slide navigation + trilingual i18n.
// External file (not inline) because the site CSP is script-src 'self'.
const I18N = {
uz: {
  "nav.back":"← Frozen City","hint.keys":"← → yoki aylantiring",
  "c.tag":"game.twelfth.uz — hozir jonli","c.eyebrow":"Investor pitch",
  "c.lead":"Abadiy qishda birgalikda omon qolish shahar-qurish o'yini. Bitta Rust kod bazasi — desktop, brauzer va telefonda.",
  "c.m1":"Janr — <b>Kooperativ survival city-builder</b>","c.m2":"Texnologiya — <b>Rust + Bevy</b>","c.m3":"Holat — <b>Jonli, o'ynash mumkin</b>",
  "s2.eyebrow":"Muammo","s2.h2":"Janr talabni isbotladi — keyin uni uchga bo'lib yubordi.",
  "s2.c1.t":"Desktop klassikalar","s2.c1.d":"Frostpunk, Banished — chuqur, lekin yakka va o'rnatish shart.",
  "s2.c2.t":"Mobil klonlar","s2.c2.d":"Ulkan auditoriya, lekin sayoz yakka idle-tsikllar.",
  "s2.c3.t":"Bo'shliq","s2.c3.d":"Havola bosib, o'sha shaharni birga qurish — janrda deyarli yo'q.",
  "s3.eyebrow":"Yechim","s3.h2":"Bitta simulyatsiya — har platformada.",
  "s3.b1.t":"1 kod bazasi","s3.b1.d":"Desktop nativ · brauzer (WebGPU/WebGL2) · mobil brauzer",
  "s3.b2.t":"Server-nazoratli yagona dunyo","s3.b2.d":"Hech bir qurilma progressni soxtalashtira olmaydi",
  "s3.b3.t":"Havola — va o'yinda","s3.b3.d":"O'rnatishsiz, do'stingiz shu shaharga real vaqtda qo'shiladi",
  "s4.eyebrow":"Mahsulot","s4.h2":"Bitta Pechka. Shahar birga omon qoladi — yoki birga halok bo'ladi.",
  "s4.l1.t":"To'plash","s4.l1.d":"O'tin, ko'mir, oziq-ovqat",
  "s4.l2.t":"Qurish","s4.l2.d":"Chodirdan gospitalgacha — har birida xodim",
  "s4.l3.t":"Pechkani yoqish","s4.l3.d":"Darajasi issiqlik radiusini beradi",
  "s4.l4.t":"Tunni o'tkazish","s4.l4.d":"Bo'ronlar, kasallik, og'ir qarorlar",
  "s4.n1":"bino turi","s4.n2":"kasb","s4.n3":"tex-tarmoq","s4.n4":"megaloyiha",
  "s5.eyebrow":"Farqlanish","s5.h2":"Janrda hech kim bu kombinatsiyani bermaydi.",
  "s5.th0":"Xususiyat","s5.th1":"Desktop klassikalar","s5.th2":"Mobil klonlar","s5.part":"qisman",
  "s5.r1":"Real-vaqt kooperativ","s5.r2":"1 kod bazasidan 3 platforma","s5.r3":"Havola orqali bir zumda o'ynash","s5.r4":"Simulyatsiya chuqurligi","s5.r5":"Server-nazoratli yagona dunyo",
  "s6.eyebrow":"Bugungi holat","s6.h2":"Bu reja emas — ishlab turgan tizim.",
  "s6.s1":"parallel o'yinchi — yuklama testi","s6.s2":"test fayli — deploy darvozasi","s6.s3":"web-yuklama (gzip)","s6.s4":"til — to'liq lokalizatsiya",
  "s6.note":"Akkauntlar, saqlanadigan dunyolar, avtomatik deploy-quvur, jonli iqtisod — hammasi ishlab chiqarishda.",
  "s7.eyebrow":"Texnologik poydevor","s7.h2":"Nusxa ko'chirish qiyin bo'lgan asos.",
  "s7.c1.t":"Rust + Bevy","s7.c1.d":"Bitta kod bazasi uch platformaga nativ tezlikda kompilyatsiya bo'ladi",
  "s7.c2.t":"Deterministik yadro","s7.c2.d":"Bir xil kirish — bir xil natija: serverda, brauzerda, oflaynda birdek",
  "s7.c3.t":"Delta-snapshot protokoli","s7.c3.d":"Faqat o'zgargan qism uzatiladi — mobil tarmoqda ham yengil",
  "s7.c4.t":"Protsedural grafika","s7.c4.d":"Rassom xarajatisiz — butun ko'rinish kod bilan chiziladi",
  "s8.eyebrow":"Vizyon","s8.h2":"Bitta gulxandan zamonaviy davlatgacha.",
  "badge.live":"Bugun jonli","badge.plan":"Rejada",
  "s8.e1.t":"Ibtidoiy qabila","s8.e1.r":"Olov · yog'och · tosh",
  "s8.e2.t":"Qishloq","s8.e2.r":"G'isht · ruda · chorva",
  "s8.e3.t":"Sanoat shahri","s8.e3.r":"Ko'mir · metall · elektr",
  "s8.e4.t":"Megapolis","s8.e4.r":"Hi-tech · moliya",
  "s8.cl1":"Muz","s8.cl2":"Jazirama","s8.cl3":"Toshqin",
  "s8.clnote":"— bitta kampaniyada uch iqlim inqirozi, har biri o'z infratuzilmasini talab qiladi",
  "tl.1":"Bugun — V0.16 jonli","tl.2":"Reliz — CI/CD, 5 platforma","tl.3":"O'sish — vitrina, marketing, marketplace","tl.4":"Masshtab — 1 000+ o'yinchi, ko'p mintaqa","s8b.eyebrow":"Vizyon — iqlim","s8b.h2":"Sovuq — faqat birinchi inqiroz.","s8b.cl1.d":"Sovuq urishi va ochlik — pechka, teplitsa, issiq kiyim kerak","s8b.cl2.d":"Suv taqchilligi va yong'inlar — suv ombori, irrigatsiya, sovutish","s8b.cl3.d":"Yerlar suv ostida — suzuvchi platformalar, gidroponika",
  "s9.eyebrow":"Yo'l xaritasi","s9.h2":"Keyingi investitsiya nimani sotib oladi.",
  "s9.c1.k":"Yaqin muddat","s9.c1.t":"Relizga tayyorlik","s9.c1.i1":"CI/CD — beshta platforma artefakti","s9.c1.i2":"WASM hajmini qisqartirish",
  "s9.c2.k":"O'rta muddat","s9.c2.t":"Tarqatish","s9.c2.i1":"Vitrina, treyler, marketing kanali","s9.c2.i2":"Hissa-reyestri ustida marketplace",
  "s9.c3.k":"Masshtab","s9.c3.t":"O'sish zaxirasi","s9.c3.i1":"1 000+ parallel uchun sharding","s9.c3.i2":"Ko'p mintaqali dunyo-menejeri",
  "s10.eyebrow":"Taklif","s10.h2":"Keyingi qadam — suhbat.",
  "s10.big":"Shartlar uchrashuvda kelishiladi. Mahsulot esa allaqachon jonli — hoziroq ochib, o'ynab ko'ring.",
  "s10.cta1":"O'yinni ochish","s10.cta2":"Bog'lanish — twelfth.uz","s10.cta3":"To'liq brifing"
},
en: {
  "nav.back":"← Frozen City","hint.keys":"← → or scroll",
  "c.tag":"game.twelfth.uz — live now","c.eyebrow":"Investor pitch",
  "c.lead":"A cooperative survival city-builder in the endless winter. One Rust codebase — desktop, browser and phones.",
  "c.m1":"Genre — <b>Co-op survival city-builder</b>","c.m2":"Tech — <b>Rust + Bevy</b>","c.m3":"Status — <b>Live and playable</b>",
  "s2.eyebrow":"Problem","s2.h2":"The genre proved demand — then split it three ways.",
  "s2.c1.t":"Desktop classics","s2.c1.d":"Frostpunk, Banished — deep, but single-player and installed.",
  "s2.c2.t":"Mobile clones","s2.c2.d":"Huge audience, but shallow single-player idle loops.",
  "s2.c3.t":"The gap","s2.c3.d":"Click a link, build the same city together — almost nothing offers it.",
  "s3.eyebrow":"Solution","s3.h2":"One simulation — on every platform.",
  "s3.b1.t":"1 codebase","s3.b1.d":"Native desktop · browser (WebGPU/WebGL2) · mobile browser",
  "s3.b2.t":"Server-authoritative world","s3.b2.d":"No device can fake progress",
  "s3.b3.t":"A link — and you're in","s3.b3.d":"No install; a friend joins your city in real time",
  "s4.eyebrow":"Product","s4.h2":"One Furnace. The city survives together — or falls together.",
  "s4.l1.t":"Gather","s4.l1.d":"Wood, coal, food",
  "s4.l2.t":"Build","s4.l2.d":"From tents to a hospital — each staffed",
  "s4.l3.t":"Feed the Furnace","s4.l3.d":"Its level sets the heat radius",
  "s4.l4.t":"Survive the night","s4.l4.d":"Blizzards, sickness, hard choices",
  "s4.n1":"building types","s4.n2":"professions","s4.n3":"tech branches","s4.n4":"megaproject",
  "s5.eyebrow":"Differentiation","s5.h2":"No one in the genre offers this combination.",
  "s5.th0":"Capability","s5.th1":"Desktop classics","s5.th2":"Mobile clones","s5.part":"partial",
  "s5.r1":"Real-time co-op","s5.r2":"3 platforms from 1 codebase","s5.r3":"Instant play via a link","s5.r4":"Simulation depth","s5.r5":"Server-authoritative shared world",
  "s6.eyebrow":"Traction","s6.h2":"Not a plan — a running system.",
  "s6.s1":"concurrent players — load-tested","s6.s2":"test files — the deploy gate","s6.s3":"web download (gzip)","s6.s4":"languages — fully localized",
  "s6.note":"Accounts, persistent worlds, an automated deploy pipeline, a living economy — all in production.",
  "s7.eyebrow":"Technical moat","s7.h2":"A foundation that is hard to copy.",
  "s7.c1.t":"Rust + Bevy","s7.c1.d":"One codebase compiles to three platforms at native speed",
  "s7.c2.t":"Deterministic core","s7.c2.d":"Same input, same result — on server, browser or offline",
  "s7.c3.t":"Delta-snapshot protocol","s7.c3.d":"Only changes travel — light even on mobile networks",
  "s7.c4.t":"Procedural visuals","s7.c4.d":"No art budget — the entire look is drawn by code",
  "s8.eyebrow":"Vision","s8.h2":"From one campfire to a modern state.",
  "badge.live":"Live today","badge.plan":"Planned",
  "s8.e1.t":"Primitive tribe","s8.e1.r":"Fire · wood · stone",
  "s8.e2.t":"Village","s8.e2.r":"Brick · ore · livestock",
  "s8.e3.t":"Industrial city","s8.e3.r":"Coal · metal · electricity",
  "s8.e4.t":"Metropolis","s8.e4.r":"High tech · finance",
  "s8.cl1":"Ice","s8.cl2":"Heat","s8.cl3":"Flood",
  "s8.clnote":"— three climate crises in one campaign, each demanding its own infrastructure",
  "tl.1":"Today — V0.16 live","tl.2":"Release — CI/CD, 5 platforms","tl.3":"Growth — storefront, marketing, marketplace","tl.4":"Scale — 1,000+ players, multi-region","s8b.eyebrow":"Vision — climate","s8b.h2":"Cold is only the first crisis.","s8b.cl1.d":"Frostbite and famine — demands the furnace, greenhouses, warm clothing","s8b.cl2.d":"Water scarcity and fires — reservoirs, irrigation, cooling","s8b.cl3.d":"Land goes underwater — floating platforms, hydroponics",
  "s9.eyebrow":"Roadmap","s9.h2":"What the next investment buys.",
  "s9.c1.k":"Near term","s9.c1.t":"Ship readiness","s9.c1.i1":"CI/CD — five platform artifacts","s9.c1.i2":"WASM size reduction",
  "s9.c2.k":"Mid term","s9.c2.t":"Distribution","s9.c2.i1":"Storefront, trailer, marketing channel","s9.c2.i2":"Marketplace on the contribution ledger",
  "s9.c3.k":"Scale","s9.c3.t":"Headroom","s9.c3.i1":"Sharding for 1,000+ concurrent","s9.c3.i2":"Multi-region world manager",
  "s10.eyebrow":"The ask","s10.h2":"The next step is a conversation.",
  "s10.big":"Terms are agreed in a meeting. The product is already live — open it and play right now.",
  "s10.cta1":"Open the game","s10.cta2":"Get in touch — twelfth.uz","s10.cta3":"Full briefing"
},
ru: {
  "nav.back":"← Frozen City","hint.keys":"← → или прокрутка",
  "c.tag":"game.twelfth.uz — уже работает","c.eyebrow":"Инвестиционный питч",
  "c.lead":"Кооперативный сити-билдер на выживание в бесконечную зиму. Одна кодовая база на Rust — десктоп, браузер, телефоны.",
  "c.m1":"Жанр — <b>Кооп survival-сити-билдер</b>","c.m2":"Технологии — <b>Rust + Bevy</b>","c.m3":"Статус — <b>Работает, можно играть</b>",
  "s2.eyebrow":"Проблема","s2.h2":"Жанр доказал спрос — и разделил его натрое.",
  "s2.c1.t":"Десктопная классика","s2.c1.d":"Frostpunk, Banished — глубоко, но одиночно и с установкой.",
  "s2.c2.t":"Мобильные клоны","s2.c2.d":"Огромная аудитория, но неглубокие одиночные idle-циклы.",
  "s2.c3.t":"Разрыв","s2.c3.d":"Кликнуть ссылку и строить тот же город вместе — почти никто не даёт.",
  "s3.eyebrow":"Решение","s3.h2":"Одна симуляция — на каждой платформе.",
  "s3.b1.t":"1 кодовая база","s3.b1.d":"Нативный десктоп · браузер (WebGPU/WebGL2) · мобильный браузер",
  "s3.b2.t":"Сервер-авторитативный мир","s3.b2.d":"Ни одно устройство не подделает прогресс",
  "s3.b3.t":"Ссылка — и вы в игре","s3.b3.d":"Без установки; друг присоединяется к вашему городу в реальном времени",
  "s4.eyebrow":"Продукт","s4.h2":"Одна Печь. Город выживает вместе — или гибнет вместе.",
  "s4.l1.t":"Собирать","s4.l1.d":"Дрова, уголь, еда",
  "s4.l2.t":"Строить","s4.l2.d":"От палаток до госпиталя — везде персонал",
  "s4.l3.t":"Топить печь","s4.l3.d":"Уровень задаёт радиус тепла",
  "s4.l4.t":"Пережить ночь","s4.l4.d":"Бураны, болезни, трудные решения",
  "s4.n1":"типов зданий","s4.n2":"профессий","s4.n3":"веток технологий","s4.n4":"мегапроект",
  "s5.eyebrow":"Отличие","s5.h2":"Никто в жанре не предлагает эту комбинацию.",
  "s5.th0":"Возможность","s5.th1":"Десктопная классика","s5.th2":"Мобильные клоны","s5.part":"частично",
  "s5.r1":"Кооп в реальном времени","s5.r2":"3 платформы из 1 кодовой базы","s5.r3":"Мгновенная игра по ссылке","s5.r4":"Глубина симуляции","s5.r5":"Сервер-авторитативный общий мир",
  "s6.eyebrow":"Статус","s6.h2":"Это не план — это работающая система.",
  "s6.s1":"одновременных игроков — нагрузочный тест","s6.s2":"файлов тестов — шлюз деплоя","s6.s3":"веб-загрузка (gzip)","s6.s4":"языка — полная локализация",
  "s6.note":"Аккаунты, сохраняемые миры, автодеплой, живая экономика — всё в продакшне.",
  "s7.eyebrow":"Технологическая база","s7.h2":"Основа, которую трудно скопировать.",
  "s7.c1.t":"Rust + Bevy","s7.c1.d":"Одна кодовая база компилируется на три платформы с нативной скоростью",
  "s7.c2.t":"Детерминированное ядро","s7.c2.d":"Один вход — один результат: на сервере, в браузере, офлайн",
  "s7.c3.t":"Дельта-снапшот протокол","s7.c3.d":"Передаются только изменения — легко даже в мобильной сети",
  "s7.c4.t":"Процедурная графика","s7.c4.d":"Без затрат на арт — весь облик рисуется кодом",
  "s8.eyebrow":"Видение","s8.h2":"От одного костра до современного государства.",
  "badge.live":"Уже сегодня","badge.plan":"Запланировано",
  "s8.e1.t":"Первобытное племя","s8.e1.r":"Огонь · дерево · камень",
  "s8.e2.t":"Деревня","s8.e2.r":"Кирпич · руда · скот",
  "s8.e3.t":"Промышленный город","s8.e3.r":"Уголь · металл · электричество",
  "s8.e4.t":"Мегаполис","s8.e4.r":"Хай-тек · финансы",
  "s8.cl1":"Лёд","s8.cl2":"Зной","s8.cl3":"Потоп",
  "s8.clnote":"— три климатических кризиса в одной кампании, каждый требует своей инфраструктуры",
  "tl.1":"Сегодня — V0.16 в проде","tl.2":"Релиз — CI/CD, 5 платформ","tl.3":"Рост — витрина, маркетинг, маркетплейс","tl.4":"Масштаб — 1 000+ игроков, мультирегион","s8b.eyebrow":"Видение — климат","s8b.h2":"Холод — лишь первый кризис.","s8b.cl1.d":"Обморожение и голод — нужны печь, теплицы, тёплая одежда","s8b.cl2.d":"Нехватка воды и пожары — резервуары, ирригация, охлаждение","s8b.cl3.d":"Земля уходит под воду — плавучие платформы, гидропоника",
  "s9.eyebrow":"Дорожная карта","s9.h2":"Что покупает следующая инвестиция.",
  "s9.c1.k":"Ближайший срок","s9.c1.t":"Готовность к релизу","s9.c1.i1":"CI/CD — пять платформенных артефактов","s9.c1.i2":"Сокращение размера WASM",
  "s9.c2.k":"Средний срок","s9.c2.t":"Дистрибуция","s9.c2.i1":"Витрина, трейлер, маркетинговый канал","s9.c2.i2":"Маркетплейс на реестре вкладов",
  "s9.c3.k":"Масштаб","s9.c3.t":"Запас роста","s9.c3.i1":"Шардинг для 1 000+","s9.c3.i2":"Мультирегиональный менеджер миров",
  "s10.eyebrow":"Предложение","s10.h2":"Следующий шаг — разговор.",
  "s10.big":"Условия согласуются на встрече. Продукт уже работает — откройте и играйте прямо сейчас.",
  "s10.cta1":"Открыть игру","s10.cta2":"Связаться — twelfth.uz","s10.cta3":"Полный брифинг"
}
};

(function(){
  // ---- i18n ----
  const KEY = "fc_pitch_lang";
  const langBtns = document.querySelectorAll(".lang-switch button");
  function applyLang(lang){
    if(!I18N[lang]) lang = "uz";
    const d = I18N[lang];
    document.documentElement.lang = lang;
    document.querySelectorAll("[data-i18n]").forEach(el => {
      const k = el.getAttribute("data-i18n");
      if(d[k] !== undefined) el.innerHTML = d[k];
    });
    langBtns.forEach(b => b.classList.toggle("active", b.dataset.lang === lang));
    try{ localStorage.setItem(KEY, lang); }catch(e){}
  }
  langBtns.forEach(b => b.addEventListener("click", () => applyLang(b.dataset.lang)));
  let saved = "uz";
  try{ saved = localStorage.getItem(KEY) || "uz"; }catch(e){}
  applyLang(saved);

  // ---- slide navigation ----
  const deck = document.getElementById("deck");
  const slides = Array.from(deck.querySelectorAll(".sl"));
  const dotsBox = document.getElementById("dots");
  const cur = document.getElementById("cur");
  document.getElementById("tot").textContent = String(slides.length).padStart(2, "0");

  slides.forEach((s, i) => {
    const b = document.createElement("button");
    b.setAttribute("aria-label", "Slide " + (i + 1));
    b.addEventListener("click", () => slides[i].scrollIntoView());
    dotsBox.appendChild(b);
  });
  const dots = Array.from(dotsBox.children);

  let active = 0;
  const io = new IntersectionObserver(entries => {
    entries.forEach(e => {
      if(e.isIntersecting){
        active = slides.indexOf(e.target);
        cur.textContent = String(active + 1).padStart(2, "0");
        dots.forEach((d, i) => d.classList.toggle("active", i === active));
      }
    });
  }, { root: deck, threshold: 0.6 });
  slides.forEach(s => io.observe(s));

  function go(i){
    i = Math.max(0, Math.min(slides.length - 1, i));
    slides[i].scrollIntoView();
  }
  document.addEventListener("keydown", e => {
    if(e.altKey || e.ctrlKey || e.metaKey) return;
    switch(e.key){
      case "ArrowRight": case "ArrowDown": case "PageDown": case " ":
        e.preventDefault(); go(active + 1); break;
      case "ArrowLeft": case "ArrowUp": case "PageUp":
        e.preventDefault(); go(active - 1); break;
      case "Home": e.preventDefault(); go(0); break;
      case "End": e.preventDefault(); go(slides.length - 1); break;
    }
  });
})();
