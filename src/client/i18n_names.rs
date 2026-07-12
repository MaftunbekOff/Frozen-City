//! O'yin lug'ati: bino/kasb/texnologiya/missiya nomlari va tavsiflarining
//! lokalizatsiyasi (uz/en/ru). En — fc-game'dagi mavjud `name()` matnlari;
//! sim/server matnlariga TEGILMAYDI, bu faqat klient ko'rsatuv qatlami.
//! Yangi enum varianti qo'shilsa exhaustive match kompilyatsiyada ushlaydi.

#![allow(dead_code)] // qayta-ishlash bosqichida iste'molchilar bosqichma-bosqich ulanadi

use frozen_city::game::types::{BuildingKind, MissionKind, Profession, Tech};

use super::i18n::Lang;

pub fn building_name(k: BuildingKind, l: Lang) -> &'static str {
    match (k, l) {
        (BuildingKind::Furnace, Lang::Uz) => "Pech",
        (BuildingKind::Furnace, Lang::En) => "Furnace",
        (BuildingKind::Furnace, Lang::Ru) => "Печь",
        (BuildingKind::Tent, Lang::Uz) => "Chodir",
        (BuildingKind::Tent, Lang::En) => "Tent",
        (BuildingKind::Tent, Lang::Ru) => "Палатка",
        (BuildingKind::Sawmill, Lang::Uz) => "Arraxona",
        (BuildingKind::Sawmill, Lang::En) => "Sawmill",
        (BuildingKind::Sawmill, Lang::Ru) => "Лесопилка",
        (BuildingKind::CoalMine, Lang::Uz) => "Ko'mir koni",
        (BuildingKind::CoalMine, Lang::En) => "Coal Mine",
        (BuildingKind::CoalMine, Lang::Ru) => "Угольная шахта",
        (BuildingKind::HunterHut, Lang::Uz) => "Ovchi kulbasi",
        (BuildingKind::HunterHut, Lang::En) => "Hunter's Hut",
        (BuildingKind::HunterHut, Lang::Ru) => "Хижина охотника",
        (BuildingKind::Greenhouse, Lang::Uz) => "Issiqxona",
        (BuildingKind::Greenhouse, Lang::En) => "Greenhouse",
        (BuildingKind::Greenhouse, Lang::Ru) => "Теплица",
        (BuildingKind::Hospital, Lang::Uz) => "Kasalxona",
        (BuildingKind::Hospital, Lang::En) => "Hospital",
        (BuildingKind::Hospital, Lang::Ru) => "Госпиталь",
        (BuildingKind::Kitchen, Lang::Uz) => "Oshxona",
        (BuildingKind::Kitchen, Lang::En) => "Kitchen",
        (BuildingKind::Kitchen, Lang::Ru) => "Кухня",
        (BuildingKind::Warehouse, Lang::Uz) => "Ombor",
        (BuildingKind::Warehouse, Lang::En) => "Warehouse",
        (BuildingKind::Warehouse, Lang::Ru) => "Склад",
    }
}

pub fn building_desc(k: BuildingKind, l: Lang) -> &'static str {
    match (k, l) {
        (BuildingKind::Furnace, Lang::Uz) => "O'chirmang — ko'mir (yoki yog'och) yondiradi.",
        (BuildingKind::Furnace, Lang::En) => "Keep it burning. Consumes coal (or wood).",
        (BuildingKind::Furnace, Lang::Ru) => "Не дайте ей погаснуть. Сжигает уголь (или дерево).",
        (BuildingKind::Tent, Lang::Uz) => "4 kishini sig'diradi. Issiqlik doirasi ichiga qo'ying.",
        (BuildingKind::Tent, Lang::En) => "Houses 4 people. Place inside the heat radius.",
        (BuildingKind::Tent, Lang::Ru) => "Вмещает 4 человек. Ставьте в радиусе тепла.",
        (BuildingKind::Sawmill, Lang::Uz) => "Ishchilar yaqin o'rmondan yog'och tayyorlaydi.",
        (BuildingKind::Sawmill, Lang::En) => "Workers harvest nearby forest for wood.",
        (BuildingKind::Sawmill, Lang::Ru) => "Рабочие заготавливают дерево в ближнем лесу.",
        (BuildingKind::CoalMine, Lang::Uz) => "Faqat ko'mir koni ustiga qurish mumkin.",
        (BuildingKind::CoalMine, Lang::En) => "Must be placed on a coal deposit.",
        (BuildingKind::CoalMine, Lang::Ru) => "Ставится только на угольном месторождении.",
        (BuildingKind::HunterHut, Lang::Uz) => "Ishchilar oziq uchun ov qiladi.",
        (BuildingKind::HunterHut, Lang::En) => "Workers hunt for food.",
        (BuildingKind::HunterHut, Lang::Ru) => "Рабочие добывают еду охотой.",
        (BuildingKind::Greenhouse, Lang::Uz) => "Yuqori hosilli yopiq ferma (ishchi boshiga ko'proq oziq).",
        (BuildingKind::Greenhouse, Lang::En) => "A high-output indoor farm (more food/worker).",
        (BuildingKind::Greenhouse, Lang::Ru) => "Крытая ферма с высокой отдачей (больше еды на рабочего).",
        (BuildingKind::Hospital, Lang::Uz) => "Ishchili bo'lsa: aholi tezroq sog'ayadi.",
        (BuildingKind::Hospital, Lang::En) => "Staffed: heals survivors faster.",
        (BuildingKind::Hospital, Lang::Ru) => "С персоналом: жители выздоравливают быстрее.",
        (BuildingKind::Kitchen, Lang::Uz) => "Ishchili bo'lsa: shahar oziqni tejamli iste'mol qiladi.",
        (BuildingKind::Kitchen, Lang::En) => "Staffed: the city eats more efficiently.",
        (BuildingKind::Kitchen, Lang::Ru) => "С персоналом: город расходует еду экономнее.",
        (BuildingKind::Warehouse, Lang::Uz) => "Ishchili bo'lsa: qurilishga yog'och kamroq sarflanadi.",
        (BuildingKind::Warehouse, Lang::En) => "Staffed: new construction wastes less wood.",
        (BuildingKind::Warehouse, Lang::Ru) => "С персоналом: стройка тратит меньше дерева.",
    }
}

pub fn profession_name(p: Profession, l: Lang) -> &'static str {
    match (p, l) {
        (Profession::Lumberjack, Lang::Uz) => "O'tinchi",
        (Profession::Lumberjack, Lang::En) => "Lumberjack",
        (Profession::Lumberjack, Lang::Ru) => "Дровосек",
        (Profession::Miner, Lang::Uz) => "Konchi",
        (Profession::Miner, Lang::En) => "Miner",
        (Profession::Miner, Lang::Ru) => "Шахтёр",
        (Profession::Hunter, Lang::Uz) => "Ovchi",
        (Profession::Hunter, Lang::En) => "Hunter",
        (Profession::Hunter, Lang::Ru) => "Охотник",
        (Profession::Farmer, Lang::Uz) => "Dehqon",
        (Profession::Farmer, Lang::En) => "Farmer",
        (Profession::Farmer, Lang::Ru) => "Фермер",
        (Profession::Medic, Lang::Uz) => "Shifokor",
        (Profession::Medic, Lang::En) => "Medic",
        (Profession::Medic, Lang::Ru) => "Медик",
        (Profession::Cook, Lang::Uz) => "Oshpaz",
        (Profession::Cook, Lang::En) => "Cook",
        (Profession::Cook, Lang::Ru) => "Повар",
    }
}

pub fn tech_name(t: Tech, l: Lang) -> &'static str {
    match (t, l) {
        (Tech::Insulation, Lang::Uz) => "Izolyatsiya",
        (Tech::Insulation, Lang::En) => "Insulation",
        (Tech::Insulation, Lang::Ru) => "Утепление",
        (Tech::EfficientFurnace, Lang::Uz) => "Tejamkor pech",
        (Tech::EfficientFurnace, Lang::En) => "Efficient Furnace",
        (Tech::EfficientFurnace, Lang::Ru) => "Экономная печь",
        (Tech::Tools, Lang::Uz) => "Yaxshi asboblar",
        (Tech::Tools, Lang::En) => "Better Tools",
        (Tech::Tools, Lang::Ru) => "Улучшенные инструменты",
        (Tech::Rationing, Lang::Uz) => "Me'yorlangan ratsion",
        (Tech::Rationing, Lang::En) => "Rationing",
        (Tech::Rationing, Lang::Ru) => "Нормирование",
        (Tech::Medicine, Lang::Uz) => "Tibbiyot",
        (Tech::Medicine, Lang::En) => "Medicine",
        (Tech::Medicine, Lang::Ru) => "Медицина",
    }
}

pub fn tech_desc(t: Tech, l: Lang) -> &'static str {
    match (t, l) {
        (Tech::Insulation, Lang::Uz) => "Hamma sovuqqa biroz chidamliroq bo'ladi.",
        (Tech::Insulation, Lang::En) => "Everyone shrugs off the cold a little better.",
        (Tech::Insulation, Lang::Ru) => "Все чуть легче переносят холод.",
        (Tech::EfficientFurnace, Lang::Uz) => "Pech 25% kam yoqilg'i yondiradi.",
        (Tech::EfficientFurnace, Lang::En) => "The furnace burns 25% less fuel.",
        (Tech::EfficientFurnace, Lang::Ru) => "Печь сжигает на 25% меньше топлива.",
        (Tech::Tools, Lang::Uz) => "Ishchilar 25% ko'proq ishlab chiqaradi.",
        (Tech::Tools, Lang::En) => "Workers produce 25% more.",
        (Tech::Tools, Lang::Ru) => "Рабочие производят на 25% больше.",
        (Tech::Rationing, Lang::Uz) => "Shahar 15% kam oziq iste'mol qiladi.",
        (Tech::Rationing, Lang::En) => "The city eats 15% less food.",
        (Tech::Rationing, Lang::Ru) => "Город потребляет на 15% меньше еды.",
        (Tech::Medicine, Lang::Uz) => "Kasalxonalar 50% tezroq davolaydi.",
        (Tech::Medicine, Lang::En) => "Hospitals heal 50% faster.",
        (Tech::Medicine, Lang::Ru) => "Госпитали лечат на 50% быстрее.",
    }
}

/// Missiya yorlig'i — qator formati `"{label} {target} ({cur}/{target})"`
/// bo'lgani uchun yorliqlar shu tartibda tabiiy o'qiladigan qilib tanlangan.
pub fn mission_label(m: MissionKind, l: Lang) -> &'static str {
    match (m, l) {
        (MissionKind::BuildTents(_), Lang::Uz) => "Chodir qurish:",
        (MissionKind::BuildTents(_), Lang::En) => "Build tents",
        (MissionKind::BuildTents(_), Lang::Ru) => "Построить палатки:",
        (MissionKind::Population(_), Lang::Uz) => "Aholi soni:",
        (MissionKind::Population(_), Lang::En) => "Reach population",
        (MissionKind::Population(_), Lang::Ru) => "Население:",
        (MissionKind::Sawmills(_), Lang::Uz) => "Arraxona qurish:",
        (MissionKind::Sawmills(_), Lang::En) => "Build sawmills",
        (MissionKind::Sawmills(_), Lang::Ru) => "Построить лесопилки:",
        (MissionKind::StockpileCoal(_), Lang::Uz) => "Ko'mir zaxirasi:",
        (MissionKind::StockpileCoal(_), Lang::En) => "Stockpile coal",
        (MissionKind::StockpileCoal(_), Lang::Ru) => "Запас угля:",
        (MissionKind::SurviveDays(_), Lang::Uz) => "Omon qolish (kun):",
        (MissionKind::SurviveDays(_), Lang::En) => "Survive days",
        (MissionKind::SurviveDays(_), Lang::Ru) => "Продержаться (дней):",
    }
}
