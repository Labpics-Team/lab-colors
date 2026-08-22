//! РўРµСЃС‚С‹ РіСЂР°РЅРёС†С‹ РєРѕРЅС„РёРіР°:
//! 1. Р‘Р°Р№С‚-РІ-Р±Р°Р№С‚: `resolve_named_set(labui_reference)` СЌРјРёС‚РёС‚ РёРґРµРЅС‚РёС‡РЅРѕ
//!    `resolve_set(RoleTable::default)` РїРѕ РІСЃРµРј 240 С‚РѕС‡РєР°Рј golden-РіСЂРёРґР°.
//! 2. RED-proof Р±Р°Р№С‚-РІ-Р±Р°Р№С‚: РјСѓС‚Р°С†РёСЏ РѕРґРЅРѕРіРѕ СЂРµС†РµРїС‚Р° С„РёРєСЃС‚СѓСЂС‹ СЂРѕРЅСЏРµС‚ С‚РµСЃС‚.
//! 3. Р’Р°Р»РёРґР°С‚РѕСЂ: Р·Р°-РїСЂРµРґРµР»СЊРЅРѕРµ Р·РЅР°С‡РµРЅРёРµ РљРђР–Р”РћР™ СЂСѓС‡РєРё РґР°С‘С‚ `ConfigError` +
//!    RED-proof РјСѓС‚Р°С†РёРµР№ РїСЂРµРґРµР»Р° (РІР°Р»РёРґРЅС‹Р№ vs РЅРµРІР°Р»РёРґРЅС‹Р№ РЅР° РіСЂР°РЅРёС†Рµ).
//! 4. Р›РµСЃС‚РЅРёС†Р°/Р°Р»СЊС„Р°: Ladder/AlphaAnalog РєРѕРјРїРёР»РёСЂСѓСЋС‚СЃСЏ РІ РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅС‹Рµ СЃРїРµС†РёРё;
//!    СЃРµРјРµР№РЅС‹Рµ РёСЃС‚РѕС‡РЅРёРєРё С‚РѕС‡РЅРѕ СЃРѕС…СЂР°РЅСЏСЋС‚ РєР»РёРµРЅС‚СЃРєРёРµ СЏРєРѕСЂСЏ РІРѕ РІСЃРµС… РєРѕРЅС‚РµРєСЃС‚Р°С…;
//!    Р·РЅР°С‡РµРЅС‡РµСЃРєР°СЏ СЃРІРµСЂРєР° СЃРѕ СЃС‚Р°Р±РѕРј labui РґРµСЂР¶РёС‚ РїСЂРµРґСЃС‚Р°РІРёС‚РµР»РµР№ РѕСЃС‚Р°Р»СЊРЅС‹С… РіСЂСѓРїРї.

use super::fixture::labui_reference;
use super::test_support::resolved_repr as repr;
use super::*;
use crate::ladder::LadderPosition;
use crate::semantic::Floor;
use crate::semantic::{Resolved, resolve_named_set};
use crate::{BgInput, Role, RoleTable, ViewingConditions, resolve_set};

/// Р“СЂРёРґ golden: РґРІР° VC-РїСЂРµСЃРµС‚Р° Г— С€РµСЃС‚СЊ С„РѕРЅРѕРІ вЂ” С‚РѕС‚ Р¶Рµ, С‡С‚Рѕ РІ
/// `semantic::tests::resolve_set_golden_hex_is_byte_for_byte_stable` (240 С‚РѕС‡РµРє).
fn grid() -> ([(ViewingConditions, &'static str); 2], [&'static str; 6]) {
    (
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ],
        [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ],
    )
}

/// РЎРѕР±СЂР°С‚СЊ РєР°СЂС‚Сѓ `role.key() -> hex` РёР· РґРµС„РѕР»С‚РЅРѕР№ С‚Р°Р±Р»РёС†С‹ РґР»СЏ (bg, vc).
fn default_by_key(bg: &BgInput, vc: &ViewingConditions) -> Vec<(&'static str, String)> {
    resolve_set(bg, &RoleTable::default(), vc)
        .into_iter()
        .map(|(role, res)| (role.key(), repr(&res)))
        .collect()
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// 1. Р‘Р°Р№С‚-РІ-Р±Р°Р№С‚ СЌРєРІРёРІР°Р»РµРЅС‚РЅРѕСЃС‚СЊ РЅР° РІСЃРµС… 240 С‚РѕС‡РєР°С….
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn labui_named_set_is_byte_identical_to_default_role_table() {
    let table = labui_reference().compile_named_role_table().expect(
        "СЌС‚Р°Р»РѕРЅРЅР°СЏ С„РёРєСЃС‚СѓСЂР° labui РѕР±СЏР·Р°РЅР° РєРѕРјРїРёР»РёСЂРѕРІР°С‚СЊСЃСЏ",
    );

    // Р¤РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ 20 core-СЂРѕР»РµР№ РџР›Р®РЎ СЃРµРјРµР№РЅС‹Рµ/FX/Р°Р»СЊС„Р°
    // Р»РµСЃС‚РЅРёС†Сѓ. Р‘Р°Р№С‚-РІ-Р±Р°Р№С‚ РіР°СЂР°РЅС‚РёСЏ вЂ” РЅР° РЎРћР›Р’Р•Р -СЂРѕР»СЏС… (РёРјРµРЅР° = Role::key()):
    // РёРјРµРЅРЅРѕ РёС… РїРёРЅРёС‚ golden-РіСЂРёРґ. РќРµР№С‚СЂР°Р»СЊРЅС‹Рµ Р·Р°Р»РёРІРєРё/РіСЂР°РЅРёС†С‹ РќРђРњР•Р Р•РќРќРћ
    // СЂР°СЃС…РѕРґСЏС‚СЃСЏ СЃ РґРµС„РѕР»С‚-С‚Р°Р±Р»РёС†РµР№: РѕРЅРё СЌРјРёС‚СЏС‚СЃСЏ Р»РµСЃС‚РЅРёС†РµР№ rgba(mid, О±) вЂ”
    // РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅРѕСЃС‚СЊ РѕР±СЏР·Р°РЅР° Р»РѕР¶РёС‚СЊСЃСЏ РЅР° Р»СЋР±СѓСЋ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ, СЃРѕР»РІРµСЂ-СЃРѕР»РёРґ
    // РµС‘ С‚РµСЂСЏР»; РёС… Р·РЅР°С‡РµРЅС‡РµСЃРєР°СЏ РёСЃС‚РёРЅР° вЂ” СЃРІРµСЂРєР° СЃРѕ СЃС‚Р°Р±РѕРј
    // (representative_roles_match_stub_values_light_and_dark).
    // РџР»СЋСЃ СЂРѕР»Рё, РїРѕРєРёРЅСѓРІС€РёРµ РїР°СЃРїРѕСЂС‚ РїРѕ Р·Р°РєРѕРЅСѓ СЃРµРјР°РЅС‚РёРєРё: separator вЂ” С‚РѕРєРµРЅР°
    // РЅРµС‚ (Р±РѕСЂРґРµСЂ Рё СЃРµРїР°СЂР°С‚РѕСЂ РµРґРёРЅС‹, РєРѕРјРїРѕРЅРµРЅС‚ РїСЂРёРјРµРЅСЏРµС‚ Р±РѕСЂРґРµСЂ), shadow-* вЂ”
    // РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅС‹С… Р»РµСЃС‚РЅРёС†Р° РїРѕРґ СЃС‚Р°Р±-РёРјРµРЅР°РјРё fx-shadow-* (СЃРѕР»РёРґ РЅР°Рґ РєРѕРЅС‚РµРЅС‚РѕРј Р±С‹Р» Р±С‹
    // РіСЂСЏР·СЊСЋ), border-strong вЂ” РїРѕР» СЂР°Р·Р»РёС‡РёРјРѕСЃС‚Рё AaUi (Р·РµСЂРєР°Р»РёС‚СЃСЏ: РґРµС„РѕР»С‚-С‚Р°Р±Р»РёС†Р°
    // РЅРµСЃС‘С‚ С‚РѕС‚ Р¶Рµ РїРѕР»).
    const LADDER_MIGRATED: [&str; 11] = [
        "fill-primary",
        "fill-secondary",
        "fill-tertiary",
        "fill-quaternary",
        "border-base",
        "border-soft",
        "separator",
        "shadow-minor",
        "shadow-ambient",
        "shadow-penumbra",
        "shadow-major",
    ];
    let core_keys: Vec<&'static str> = Role::ALL
        .iter()
        .map(|r| r.key())
        .filter(|k| !LADDER_MIGRATED.contains(k))
        .collect();
    for key in &core_keys {
        assert!(
            table.entries().iter().any(|(n, _)| n == key),
            "С„РёРєСЃС‚СѓСЂР° labui РѕР±СЏР·Р°РЅР° РЅРµСЃС‚Рё СЃРµРіРѕРґРЅСЏС€РЅСЋСЋ СЂРѕР»СЊ `{key}`"
        );
    }

    let (vcs, bgs) = grid();
    let mut compared = 0usize;
    for (vc, _vc_name) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let named = resolve_named_set(&bg, &table, &vc).expect(
                "РІР°Р»РёРґРЅР°СЏ labui-С„РёРєСЃС‚СѓСЂР° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ",
            );
            let default_map = default_by_key(&bg, &vc);

            // РЎСЂР°РІРЅРёРІР°РµРј РўРћР›Р¬РљРћ 19 СЃРµРіРѕРґРЅСЏС€РЅРёС… СЂРѕР»РµР№ (Р°РєС†РµРЅС‚РЅС‹Рµ вЂ” РЅРѕРІС‹Рµ, Сѓ РЅРёС… РЅРµС‚
            // РґРµС„РѕР»С‚-Р°РЅР°Р»РѕРіР°; РёС… РїРѕРєСЂС‹РІР°РµС‚ diff=РїСѓСЃС‚Рѕ С‚РµСЃС‚ РїСЂРѕС‚РёРІ consumedRoles).
            for (name, res) in &named {
                if !core_keys.contains(&name.as_str()) {
                    continue;
                }
                let got = repr(res);
                let want = default_map
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, hex)| hex.clone())
                    .unwrap_or_else(|| {
                        panic!("РЅРµС‚ РґРµС„РѕР»С‚РЅРѕР№ СЂРѕР»Рё СЃ РєР»СЋС‡РѕРј `{name}`")
                    });
                assert_eq!(
                    got, want,
                    "Р‘РђР™Рў-Р”Р РР¤Рў {bg_hex}/{_vc_name} `{name}`: config={got}, default={want}"
                );
                compared += 1;
            }
        }
    }
    // 8 СЃРѕР»РІРµСЂ-СЂРѕР»РµР№ (19 в€’ 6 Р»РµСЃС‚РЅРёС‡РЅС‹С… в€’ separator в€’ 4 С‚РµРЅРµР№, СѓС€РµРґС€РёС… РёР·
    // РїР°СЃРїРѕСЂС‚Р° РїРѕ Р·Р°РєРѕРЅСѓ СЃРµРјР°РЅС‚РёРєРё; СЃР»РѕРІР°СЂРЅС‹Р№ РєР°РЅРѕРЅ #92 СЃРЅС‘СЃ СЂРѕР»СЊ icon) Г—
    // 2 VC Г— 6 С„РѕРЅРѕРІ = 96.
    assert_eq!(
        compared, 96,
        "РґРѕР»Р¶РЅРѕ СЃСЂР°РІРЅРёС‚СЊСЃСЏ СЂРѕРІРЅРѕ 96 СЃРѕР»РІРµСЂ-С‚РѕС‡РµРє (РїРёРЅ РЅРµ РІР°РєСѓСѓРјРЅС‹Р№)"
    );
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// 2. RED-proof Р±Р°Р№С‚-РІ-Р±Р°Р№С‚: РјСѓС‚Р°С†РёСЏ СЂРµС†РµРїС‚Р° С„РёРєСЃС‚СѓСЂС‹ СЂРѕРЅСЏРµС‚ С‚РµСЃС‚.
//    Р”РѕРєР°Р·С‹РІР°РµС‚, С‡С‚Рѕ С‚РµСЃС‚ РІС‹С€Рµ РљРЈРЎРђР•РўРЎРЇ (РЅРµ green-from-birth).
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn byte_identity_test_bites_on_mutated_recipe() {
    // РњСѓС‚РёСЂСѓРµРј РћР”РРќ СЂРµС†РµРїС‚: label-primary fraction 0.968 в†’ 0.627 (РєРѕРЅС‚СЂР°РєС‚
    // secondary). Р­РјРёСЃСЃРёСЏ label-primary РѕР±СЏР·Р°РЅР° СЂР°Р·РѕР№С‚РёСЃСЊ СЃ РґРµС„РѕР»С‚РѕРј С…РѕС‚СЏ Р±С‹ РЅР°
    // РѕРґРЅРѕРј С„РѕРЅРµ вЂ” РёРЅР°С‡Рµ Р±Р°Р№С‚-РІ-Р±Р°Р№С‚ С‚РµСЃС‚ Р±С‹Р» Р±С‹ СЃР»РµРї Рє СЂРµС†РµРїС‚Сѓ.
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "label-primary" {
            *recipe = RoleRecipe::TextAnchor {
                fraction: 0.627,
                floor: Floor::AaText,
                hue: None,
            };
        }
    }
    let mutated = cfg
        .compile_named_role_table()
        .expect("РјСѓС‚Р°РЅС‚ РІСЃС‘ РµС‰С‘ РІР°Р»РёРґРµРЅ (fraction РІ РїСЂРµРґРµР»Р°С…)");

    let (vcs, bgs) = grid();
    let mut any_diff = false;
    for (vc, _n) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let named = resolve_named_set(&bg, &mutated, &vc)
                .expect("РІР°Р»РёРґРЅС‹Р№ recipe-РјСѓС‚Р°РЅС‚ РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
            let default_map = default_by_key(&bg, &vc);
            for (name, res) in &named {
                if name == "label-primary" {
                    let got = repr(res);
                    let want = default_map
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, hex)| hex.clone())
                        .unwrap();
                    if got != want {
                        any_diff = true;
                    }
                }
            }
        }
    }
    assert!(
        any_diff,
        "RED-proof РїСЂРѕРІР°Р»РµРЅ: РјСѓС‚Р°С†РёСЏ СЂРµС†РµРїС‚Р° label-primary РќР• РёР·РјРµРЅРёР»Р° СЌРјРёСЃСЃРёСЋ вЂ” \
         Р±Р°Р№С‚-РІ-Р±Р°Р№С‚ С‚РµСЃС‚ Р±С‹Р» Р±С‹ СЃР»РµРї"
    );
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// 3. Р’Р°Р»РёРґР°С‚РѕСЂ: СЌС‚Р°Р»РѕРЅ РІР°Р»РёРґРµРЅ; РєР°Р¶РґР°СЏ СЂСѓС‡РєР° Р·Р° РїСЂРµРґРµР»РѕРј РґР°С‘С‚ ConfigError.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn labui_reference_passes_validation() {
    assert_eq!(labui_reference().validate(), Ok(()));
}

/// РњСѓС‚РёСЂРѕРІР°С‚СЊ РїРµСЂРІС‹Р№ СЂРµС†РµРїС‚ РґР°РЅРЅРѕРіРѕ РІРёРґР° Рё РІРµСЂРЅСѓС‚СЊ РєРѕРЅС„РёРі.
fn with_role_recipe(name: &str, recipe: RoleRecipe) -> ThemeConfig {
    let mut cfg = labui_reference();
    let entry = cfg
        .roles
        .iter_mut()
        .find(|(rname, _)| rname == name)
        .unwrap_or_else(|| panic!("СЂРѕР»СЊ `{name}` РѕС‚СЃСѓС‚СЃС‚РІСѓРµС‚ РІ С„РёРєСЃС‚СѓСЂРµ"));
    entry.1 = recipe;
    cfg
}

#[test]
fn fraction_out_of_bounds_is_rejected() {
    // > 1 РѕС‚РєР»РѕРЅСЏРµС‚СЃСЏ.
    let over = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.5,
            floor: Floor::AaText,
            hue: None,
        },
    );
    assert!(matches!(
        over.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.label-primary.fraction"
    ));
    // в‰¤ 0 РѕС‚РєР»РѕРЅСЏРµС‚СЃСЏ.
    let zero = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 0.0,
            floor: Floor::AaText,
            hue: None,
        },
    );
    assert!(matches!(
        zero.validate(),
        Err(ConfigError::OutOfBounds { .. })
    ));
}

#[test]
fn fraction_bound_red_proof_at_edges() {
    // RED-proof РїСЂРµРґРµР»Р°: 1.0 РІР°Р»РёРґРµРЅ (РІРµСЂС…РЅСЏСЏ РіСЂР°РЅРёС†Р° РІРєР»СЋС‡РёС‚РµР»СЊРЅР°), 1.0+Оµ вЂ” РЅРµС‚.
    let at = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.0,
            floor: Floor::AaText,
            hue: None,
        },
    );
    assert_eq!(
        at.validate(),
        Ok(()),
        "fraction=1.0 РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ РІР°Р»РёРґРµРЅ"
    );
    let over = with_role_recipe(
        "label-primary",
        RoleRecipe::TextAnchor {
            fraction: 1.0 + 1e-9,
            floor: Floor::AaText,
            hue: None,
        },
    );
    assert!(
        over.validate().is_err(),
        "fraction С‡СѓС‚СЊ РІС‹С€Рµ 1.0 РѕР±СЏР·Р°РЅ СѓРїР°СЃС‚СЊ вЂ” РёРЅР°С‡Рµ РїСЂРµРґРµР» РЅРµ РєСѓСЃР°РµС‚СЃСЏ"
    );
}

#[test]
fn dj_anchor_non_positive_is_rejected() {
    for recipe in [
        RoleRecipe::DjAnchor {
            light: 0.0,
            dark: 5.0,
        },
        RoleRecipe::DjAnchor {
            light: 5.0,
            dark: -1.0,
        },
    ] {
        let cfg = with_role_recipe("fill-primary", recipe);
        assert!(
            matches!(cfg.validate(), Err(ConfigError::OutOfBounds { .. })),
            "РЅСѓР»РµРІРѕР№/РѕС‚СЂРёС†Р°С‚РµР»СЊРЅС‹Р№ dJ' РѕР±СЏР·Р°РЅ РѕС‚РєР»РѕРЅСЏС‚СЊСЃСЏ"
        );
    }
}

#[test]
fn dj_anchor_bound_red_proof() {
    // РЎС‚СЂРѕРіРѕ РїРѕР»РѕР¶РёС‚РµР»СЊРЅС‹Р№ РїСЂРµРґРµР»: +Оµ РІР°Р»РёРґРµРЅ, 0.0 вЂ” РЅРµС‚.
    let ok = with_role_recipe(
        "fill-primary",
        RoleRecipe::DjAnchor {
            light: f64::MIN_POSITIVE,
            dark: 1.0,
        },
    );
    assert_eq!(ok.validate(), Ok(()));
    let bad = with_role_recipe(
        "fill-primary",
        RoleRecipe::DjAnchor {
            light: 0.0,
            dark: 1.0,
        },
    );
    assert!(bad.validate().is_err());
}

#[test]
fn decorative_lc_requires_the_core_physical_floor() {
    // РџСЂРѕРІРµСЂСЏРµРј С‚РѕС‡РЅСѓСЋ Р·Р°РєСЂС‹С‚СѓСЋ РіСЂР°РЅРёС†Сѓ: Р±Р»РёР¶Р°Р№С€РёР№ РјРµРЅСЊС€РёР№ binary64 СѓР¶Рµ
    // РЅРµРґРѕРјРµРЅРµРЅ, СЃР°Рј С„РёР·РёС‡РµСЃРєРёР№ РїРѕР» РїСЂРёРЅРёРјР°РµС‚СЃСЏ Р±РµР· РїРµСЂРµРїРёСЃРё.
    let below = f64::from_bits(DECORATIVE_FLOOR_MIN.to_bits() - 1);
    for magnitude in [f64::NAN, f64::INFINITY, 0.0, below] {
        let cfg = with_role_recipe("label-tertiary", RoleRecipe::DecorativeLc { magnitude });
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::OutOfBounds { handle, .. })
                    if handle == "roles.label-tertiary.magnitude"
            ),
            "magnitude={magnitude} РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РѕС‚РєР»РѕРЅРµРЅР°"
        );
    }

    let below_error = with_role_recipe(
        "label-tertiary",
        RoleRecipe::DecorativeLc { magnitude: below },
    )
    .validate()
    .expect_err("Р·РЅР°С‡РµРЅРёРµ РЅРёР¶Рµ С„РёР·РёС‡РµСЃРєРѕРіРѕ РїРѕР»Р° РѕР±СЏР·Р°РЅРѕ Р±С‹С‚СЊ РѕС‚РєР»РѕРЅРµРЅРѕ");
    let ConfigError::OutOfBounds { bound, .. } = below_error else {
        panic!(
            "РѕР¶РёРґР°Р»Р°СЃСЊ С‡РёСЃР»РѕРІР°СЏ РіСЂР°РЅРёС†Р° РґРµРєРѕСЂР°С‚РёРІРЅРѕРіРѕ РєРѕРЅС‚СЂР°СЃС‚Р°"
        );
    };
    assert_eq!(
        bound,
        format!(
            "magnitude в‰Ґ {DECORATIVE_FLOOR_MIN} Lc (РіСЂР°РЅРёС†Р° РґРµРєРѕСЂР°С‚РёРІРЅРѕР№ Lc-С†РµР»Рё)"
        )
    );
    assert!(
        !bound.contains("DECORATIVE_FLOOR_MIN"),
        "РїСѓР±Р»РёС‡РЅР°СЏ РѕС€РёР±РєР° РЅРµ РґРѕР»Р¶РЅР° РїРѕРєР°Р·С‹РІР°С‚СЊ РІРЅСѓС‚СЂРµРЅРЅРёР№ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ: {bound}"
    );

    let boundary = with_role_recipe(
        "label-tertiary",
        RoleRecipe::DecorativeLc {
            magnitude: DECORATIVE_FLOOR_MIN,
        },
    );
    assert_eq!(boundary.validate(), Ok(()));
}

#[test]
fn target_mp_non_positive_is_rejected() {
    let mut cfg = labui_reference();
    cfg.neutral.tint.target_mp = 0.0;
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::OutOfBounds { handle, .. }) if handle == "neutral.tint.target_mp"
    ));
}

#[test]
fn hue_stiffness_negative_is_rejected() {
    let mut cfg = labui_reference();
    cfg.neutral.tint.hue_stiffness = -1.0;
    assert!(cfg.validate().is_err());
    // RED-proof: 0.0 РІР°Р»РёРґРµРЅ (РЅРёР¶РЅСЏСЏ РіСЂР°РЅРёС†Р° РІРєР»СЋС‡РёС‚РµР»СЊРЅР°).
    let mut zero = labui_reference();
    zero.neutral.tint.hue_stiffness = 0.0;
    assert_eq!(zero.validate(), Ok(()));
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// Р’Р°Р»РёРґР°С‚РѕСЂ: hex / РёРјРµРЅР° / СЃСЃС‹Р»РєРё.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn invalid_hex_is_rejected() {
    let mut cfg = labui_reference();
    cfg.brand.anchors.light = "not-a-hex".to_string();
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidHex { field, .. }) if field == "brand.anchors.light"
    ));
    let mut neut = labui_reference();
    neut.neutral.anchors.dark = "#GGGGGG".to_string();
    assert!(matches!(
        neut.validate(),
        Err(ConfigError::InvalidHex { .. })
    ));
}

#[test]
fn invalid_role_name_is_rejected() {
    let mut cfg = labui_reference();
    // Р—Р°РіР»Р°РІРЅС‹Рµ Р±СѓРєРІС‹ РЅРµРґРѕРїСѓСЃС‚РёРјС‹ ([a-z0-9-]+).
    cfg.roles.push((
        "Label_Bad".to_string(),
        RoleRecipe::TextAnchor {
            fraction: 0.5,
            floor: Floor::AaUi,
            hue: None,
        },
    ));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::InvalidName { .. })
    ));
}

#[test]
fn alias_to_missing_role_is_rejected() {
    let mut cfg = labui_reference();
    // РЈРЅРёРєР°Р»СЊРЅРѕРµ РёРјСЏ Р°Р»РёР°СЃР°: РґСѓР±Р»РёРєР°С‚ СЃСѓС‰РµСЃС‚РІСѓСЋС‰РµРіРѕ РїРѕР№РјР°Р»СЃСЏ Р±С‹ СЂР°РЅСЊС€Рµ РєР°Рє
    // DuplicateKey вЂ” Р·РґРµСЃСЊ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ РёРјРµРЅРЅРѕ СЂР°Р·Р»РёС‡РёРјР°СЏ РѕС€РёР±РєР° СЃСЃС‹Р»РєРё.
    cfg.aliases
        .push(("probe-unique-alias".to_string(), "no-such-role".to_string()));
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownRole { role, .. }) if role == "no-such-role"
    ));
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// 4. Р§РµСЃС‚РЅС‹Рµ Р·Р°РіР»СѓС€РєРё РЅРµСЂРµР°Р»РёР·РѕРІР°РЅРЅС‹С… СЂРµС†РµРїС‚РѕРІ.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn ladder_recipe_compiles_to_translucent_spec() {
    // Ladder вЂ” РЅРµ Р·Р°РіР»СѓС€РєР°: РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ РІ RoleSpec::Ladder.
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Brand,
            position: LadderPosition::FillPrimary,
            floor: None,
        },
    );
    assert_eq!(cfg.validate(), Ok(()));
    let table = cfg
        .compile_named_role_table()
        .expect("Ladder РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    let (_, spec) = table
        .entries()
        .iter()
        .find(|(n, _)| n == "fill-primary")
        .unwrap();
    assert!(
        matches!(spec, RoleSpec::Ladder { alpha_light, alpha_dark, .. }
            if (*alpha_light - 0.122).abs() < 1e-12 && (*alpha_dark - 0.122).abs() < 1e-12),
        "Ladder(FillPrimary) РѕР±СЏР·Р°РЅ РЅРµСЃС‚Рё Р°Р»СЊС„Сѓ @12 (РѕР±Рµ С‚РµРјС‹); РїРѕР»СѓС‡РµРЅРѕ {spec:?}"
    );
}

#[test]
fn ladder_floor_is_valid_only_for_a_solid_readability_constraint() {
    for (position, floor) in [
        (LadderPosition::FillPrimary, Some(Floor::AaUi)),
        (LadderPosition::BorderStrong, Some(Floor::None)),
    ] {
        let cfg = with_role_recipe(
            "fill-primary",
            RoleRecipe::Ladder {
                source: LadderSource::Brand,
                position,
                floor,
            },
        );
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::InvalidLadderFloor { role, .. }) if role == "fill-primary"
        ));
    }

    let valid = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Brand,
            position: LadderPosition::BorderStrong,
            floor: Some(Floor::AaUi),
        },
    );
    assert_eq!(valid.validate(), Ok(()));
}

#[test]
fn alpha_analog_recipe_compiles_to_translucent_spec() {
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::AlphaAnalog {
            of: LadderSource::Brand,
            alpha: 0.122,
        },
    );
    assert_eq!(cfg.validate(), Ok(()));
    let table = cfg
        .compile_named_role_table()
        .expect("AlphaAnalog РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    let (_, spec) = table
        .entries()
        .iter()
        .find(|(n, _)| n == "fill-primary")
        .unwrap();
    assert!(
        matches!(spec, RoleSpec::AlphaAnalog { alpha, .. } if (*alpha - 0.122).abs() < 1e-12),
        "AlphaAnalog РѕР±СЏР·Р°РЅ РЅРµСЃС‚Рё Р·Р°РїСЂРѕС€РµРЅРЅСѓСЋ Р°Р»СЊС„Сѓ; РїРѕР»СѓС‡РµРЅРѕ {spec:?}"
    );
}

#[test]
fn ladder_source_referencing_missing_family_is_rejected() {
    let cfg = with_role_recipe(
        "fill-primary",
        RoleRecipe::Ladder {
            source: LadderSource::Family("nonexistent".to_string()),
            position: LadderPosition::FillPrimary,
            floor: None,
        },
    );
    assert!(matches!(
        cfg.validate(),
        Err(ConfigError::UnknownFamily { family, .. }) if family == "nonexistent"
    ));
}

#[test]
fn alpha_analog_alpha_out_of_bounds_is_rejected() {
    for bad in [0.0, 1.5] {
        let cfg = with_role_recipe(
            "fill-primary",
            RoleRecipe::AlphaAnalog {
                of: LadderSource::Brand,
                alpha: bad,
            },
        );
        assert!(
            matches!(cfg.validate(), Err(ConfigError::OutOfBounds { .. })),
            "alpha={bad} РѕР±СЏР·Р°РЅР° РѕС‚РєР»РѕРЅСЏС‚СЊСЃСЏ"
        );
    }
}

#[test]
fn config_error_display_is_russian_and_informative() {
    let err = ConfigError::OutOfBounds {
        handle: "roles.x.fraction".to_string(),
        value: 2.0,
        bound: "0 < fraction в‰¤ 1",
    };
    let s = err.to_string();
    assert!(s.contains("roles.x.fraction"));
    assert!(s.contains("РІРЅРµ РїСЂРµРґРµР»Р°"));
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// diff=РїСѓСЃС‚Рѕ РїСЂРѕС‚РёРІ consumedRoles labui.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// Р—Р°РјРѕСЂРѕР¶РµРЅРЅС‹Р№ snapshot РѕР¶РёРґР°РµРјС‹С… `--lab-*` РёРјС‘РЅ СЂРµС„РµСЂРµРЅСЃ-С„РёРєСЃС‚СѓСЂС‹.
/// Р­С‚Рѕ test oracle, РЅРµ SSOT РїСѓР±Р»РёС‡РЅРѕРіРѕ РєР»РёРµРЅС‚Р°: production Core РЅРµ С‡РёС‚Р°РµС‚ РµРіРѕ,
/// Р° Р°РєС‚СѓР°Р»СЊРЅРѕСЃС‚СЊ РґРѕРєР°Р·С‹РІР°РµС‚ С‚РѕР»СЊРєРѕ diff СЃ С‚РµРєСѓС‰РµР№ fixture-СЌРјРёСЃСЃРёРµР№.
/// РРјРµРЅР° Р±РµР· РїСЂРµС„РёРєСЃР° `--lab-`; IC-СЂРµР¶РёРјС‹ РЅРµ РґРѕР±Р°РІР»СЏСЋС‚ РѕС‚РґРµР»СЊРЅС‹Рµ РёРјРµРЅР°.
const LABUI_CONSUMED_ROLES: &[&str] = &[
    // Backgrounds вЂ” Р’РҐРћР”Р« (РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ), РЅРµ СЂРѕР»Рё СЌРјРёСЃСЃРёРё.
    // Labels (core neutral).
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
    // Labels вЂ” Р±СЂРµРЅРґ Рё РєР»РёРµРЅС‚СЃРєРёРµ СЃРµРјРµР№СЃС‚РІР°.
    "label-brand-primary",
    "label-brand-secondary",
    "label-brand-tertiary",
    "label-brand-quaternary",
    "label-danger-primary",
    "label-danger-secondary",
    "label-danger-tertiary",
    "label-danger-quaternary",
    "label-warning-primary",
    "label-warning-secondary",
    "label-warning-tertiary",
    "label-warning-quaternary",
    "label-success-primary",
    "label-success-secondary",
    "label-success-tertiary",
    "label-success-quaternary",
    "label-info-primary",
    "label-info-secondary",
    "label-info-tertiary",
    "label-info-quaternary",
    // Fills (core neutral).
    "fill-primary",
    "fill-secondary",
    "fill-tertiary",
    "fill-quaternary",
    "fill-none",
    // Fills вЂ” Р±СЂРµРЅРґ Рё РєР»РёРµРЅС‚СЃРєРёРµ СЃРµРјРµР№СЃС‚РІР°.
    "fill-brand-primary",
    "fill-brand-secondary",
    "fill-brand-tertiary",
    "fill-brand-quaternary",
    "fill-danger-primary",
    "fill-danger-secondary",
    "fill-danger-tertiary",
    "fill-danger-quaternary",
    "fill-warning-primary",
    "fill-warning-secondary",
    "fill-warning-tertiary",
    "fill-warning-quaternary",
    "fill-success-primary",
    "fill-success-secondary",
    "fill-success-tertiary",
    "fill-success-quaternary",
    "fill-info-primary",
    "fill-info-secondary",
    "fill-info-tertiary",
    "fill-info-quaternary",
    // Border (core neutral). border-ghost вЂ” deprecated-Р°Р»РёР°СЃ РєР°РЅРѕРЅР° #92,
    // border-none вЂ” С‡РµСЃС‚РЅС‹Р№ РЅРѕР»СЊ (РѕР±Р° РІ РєРѕРЅС‚СЂР°РєС‚Рµ roles.json labui).
    "border-strong",
    "border-base",
    "border-soft",
    "border-ghost",
    "border-none",
    // Borders вЂ” Р±СЂРµРЅРґ Рё РєР»РёРµРЅС‚СЃРєРёРµ СЃРµРјРµР№СЃС‚РІР°.
    "border-brand-strong",
    "border-brand-base",
    "border-brand-soft",
    "border-danger-strong",
    "border-danger-base",
    "border-danger-soft",
    "border-warning-strong",
    "border-warning-base",
    "border-warning-soft",
    "border-success-strong",
    "border-success-base",
    "border-success-soft",
    "border-info-strong",
    "border-info-base",
    "border-info-soft",
    // FX (РЅРµ-С‚РµРЅРµРІС‹Рµ).
    "fx-focus-ring-brand",
    "fx-focus-ring-danger",
    "fx-focus-ring-warning",
    "fx-focus-ring-neutral",
    "fx-glow-brand",
    "fx-glow-danger",
    "fx-glow-warning",
    "fx-glow-neutral",
    "fx-glow-inverted",
    "fx-skeleton-base",
    "fx-skeleton-highlight",
    // FX shadow вЂ” РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅС‹С… Р»РµСЃС‚РЅРёС†Р° С‚С‘РјРЅРѕРіРѕ СЏРєРѕСЂСЏ РїРѕРґ СЃС‚Р°Р±-РёРјРµРЅР°РјРё.
    "fx-shadow-minor",
    "fx-shadow-ambient",
    "fx-shadow-penumbra",
    "fx-shadow-major",
    // Component.
    "fill-neutral",
    "fill-accent-tinted",
    "fill-neutral-tinted",
    "fill-danger-tinted",
    "label-accent",
    "label-danger",
    "border-accent",
    "border-neutral",
    "border-danger",
    "border-focus",
    // РџСЂРѕС‡РёРµ СЌРјРёС‚РёСЂСѓРµРјС‹Рµ РЅРµР№С‚СЂР°Р»СЊРЅС‹Рµ (none вЂ” core; icon СЃРЅСЏС‚ СЃ РєРѕРЅС‚СЂР°РєС‚Р° РєР°РЅРѕРЅРѕРј
    // #92 вЂ” РіР»РёС„ РєСЂР°СЃРёС‚СЃСЏ label-tertiary; separator РќР• С‚РѕРєРµРЅ: Р±РѕСЂРґРµСЂ Рё СЃРµРїР°СЂР°С‚РѕСЂ
    // РµРґРёРЅС‹, РєРѕРјРїРѕРЅРµРЅС‚ РїСЂРёРјРµРЅСЏРµС‚ Р±РѕСЂРґРµСЂ-С‚РѕРєРµРЅ).
    "none",
];

/// Р РѕР»Рё consumedRoles labui, РЈР”РђР›РЇР•РњР«Р• РїРѕ РєРѕР»Р»Р°РїСЃСѓ РєРѕРЅС‚СЂР°РєС‚Р° (inventory В§4):
/// РєР°Р¶РґР°СЏ СЃ РїСЂРёС‡РёРЅРѕР№. Diff-С‚РµСЃС‚ РёСЃРєР»СЋС‡Р°РµС‚ РёС… РёР· С‚СЂРµР±СѓРµРјРѕРіРѕ РїРѕРєСЂС‹С‚РёСЏ вЂ” РѕРЅРё РЅРµ
/// СЌРјРёС‚РёСЂСѓСЋС‚СЃСЏ РґРІРёР¶РєРѕРј (СЂРѕР»СЊ СЂРµС€Р°РµС‚СЃСЏ РѕС‚ С„Р°РєС‚РёС‡РµСЃРєРѕРіРѕ С„РѕРЅР° / РјР°С‚РµСЂРёР°Р» = С„Р»Р°Рі).
const COLLAPSED_ROLES: &[(&str, &str)] = &[
    // РњР°С‚РµСЂРёР°Р» = Р¤Р›РђР“ С„РѕРЅР° (Backgrounds+Materials СЃС…Р»РѕРїРЅСѓС‚С‹), РЅРµ СЂРѕР»СЊ СЌРјРёСЃСЃРёРё.
    (
        "bg-material-*",
        "РјР°С‚РµСЂРёР°Р» = С„Р»Р°Рі С„РѕРЅР°, РЅРµ СЂРѕР»СЊ",
    ),
    // Р РѕР»СЊ СЂРµС€Р°РµС‚СЃСЏ РѕС‚ Р¤РђРљРўРР§Р•РЎРљРћР“Рћ С„РѕРЅР° вЂ” static-*/inverted-* РЅРµ РЅСѓР¶РЅС‹.
    (
        "*-static-dark-*",
        "СЂРѕР»СЊ РѕС‚ С„РѕРЅР°: СЃС‚Р°С‚РёРє-С‚С‘РјРЅС‹Р№ С„РѕРЅ = РІС…РѕРґ",
    ),
    (
        "*-static-light-*",
        "СЂРѕР»СЊ РѕС‚ С„РѕРЅР°: СЃС‚Р°С‚РёРє-СЃРІРµС‚Р»С‹Р№ С„РѕРЅ = РІС…РѕРґ",
    ),
    (
        "label-inverted-*",
        "СЂРѕР»СЊ РѕС‚ С„РѕРЅР°: РёРЅРІРµСЂСЃРёСЏ = РІС…РѕРґ-С„РѕРЅ",
    ),
    (
        "border-inverted",
        "СЂРѕР»СЊ РѕС‚ С„РѕРЅР°: РёРЅРІРµСЂСЃРёСЏ = РІС…РѕРґ-С„РѕРЅ",
    ),
    // on-* Р»РµР№Р±Р»С‹ РІС‹Р±СЂРѕС€РµРЅС‹ (СЃРѕР»РІРµСЂ РѕС‚ С„РѕРЅР° СЃРЅРёР·Сѓ, 36в†’~4).
    (
        "label-on-accent",
        "on-* РІС‹Р±СЂРѕС€РµРЅС‹: Р»РµР№Р±Р» СЂРµС€Р°РµС‚СЃСЏ РѕС‚ С„РѕРЅР°",
    ),
    (
        "label-on-neutral",
        "on-* РІС‹Р±СЂРѕС€РµРЅС‹: Р»РµР№Р±Р» СЂРµС€Р°РµС‚СЃСЏ РѕС‚ С„РѕРЅР°",
    ),
    (
        "label-on-danger",
        "on-* РІС‹Р±СЂРѕС€РµРЅС‹: Р»РµР№Р±Р» СЂРµС€Р°РµС‚СЃСЏ РѕС‚ С„РѕРЅР°",
    ),
    // Р¤РѕРЅС‹/РѕРІРµСЂР»РµРё вЂ” Р’РҐРћР”Р« (РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ) РёР»Рё alpha.rs-СЂРѕР»Рё.
    // Р‘Р°Р·РѕРІС‹Р№ С„РѕРЅ РѕСЃС‚Р°С‘С‚СЃСЏ Р’РҐРћР”РћРњ
    // (bg-primary/secondary/... вЂ” РјР°РїРїРёРЅРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ РЅР° С‚РѕРЅР°), РЅРѕ РІС‹РІРµРґРµРЅРЅС‹Рµ
    // РўРћРќРђ Р»РµСЃС‚РЅРёС†С‹ С„РѕРЅРѕРІ (bg-tone-*) вЂ” Р»РµРіРёС‚РёРјРЅС‹Рµ dJ'-СЌРјРёСЃСЃРёРё СЃРѕР»РІРµСЂР°:
    // В«РµР»Рµ РѕС‚Р»РёС‡РёРјРѕВ»-СЃС‚СѓРїРµРЅРё вЂ” РєРѕРЅС‚СЂР°РєС‚ РґРІРёР¶РєР°, РЅРµ СЂСѓРєРѕРїРёСЃРЅС‹Рµ hex РїРѕС‚СЂРµР±РёС‚РµР»СЏ.
    (
        "bg-primary*",
        "РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ, РЅРµ СЂРѕР»СЊ СЌРјРёСЃСЃРёРё",
    ),
    (
        "bg-secondary*",
        "РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ, РЅРµ СЂРѕР»СЊ СЌРјРёСЃСЃРёРё",
    ),
    (
        "bg-tertiary*",
        "РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ, РЅРµ СЂРѕР»СЊ СЌРјРёСЃСЃРёРё",
    ),
    (
        "bg-grouped-*",
        "РЅР°Р±РѕСЂ С„РѕРЅРѕРІ = РєРѕРЅС„РёРі РїРѕС‚СЂРµР±РёС‚РµР»СЏ, РЅРµ СЂРѕР»СЊ СЌРјРёСЃСЃРёРё",
    ),
    (
        "bg-overlay-*",
        "РѕРІРµСЂР»РµРё в†’ alpha.rs-СЂРѕР»Рё (РІРЅРµ РїРѕРіР»РѕС‰Р°РµРјРѕРіРѕ GAP)",
    ),
    // РљРѕРјРїРѕРЅРµРЅС‚РЅР°СЏ РєРѕРјРїРѕР·РёС†РёСЏ РїСЂРёРЅР°РґР»РµР¶РёС‚ РєР»РёРµРЅС‚СЃРєРѕРјСѓ Program, Р° РЅРµ Р·Р°РєСЂС‹С‚РѕРјСѓ
    // СЂРѕР»РµРІРѕРјСѓ РјРµРЅСЋ Core.
    ("badge-*", "client-owned Program composition"),
    ("fill-accent", "client-owned alias"),
    ("fill-danger", "client-owned alias"),
    (
        "control-bg",
        "РєРѕРјРїРѕРЅРµРЅС‚РЅС‹Р№ Р°Р»РёР°СЃ, РЅРµ СЂРµС†РµРїС‚ СЌРјРёСЃСЃРёРё",
    ),
];

/// diff = РџРЈРЎРўРћ: РєР°Р¶РґР°СЏ consumedRole labui (РјРёРЅСѓСЃ СѓРґР°Р»СЏРµРјС‹Рµ РїРѕ РєРѕР»Р»Р°РїСЃСѓ)
/// СЌРјРёС‚РёСЂСѓРµС‚СЃСЏ С„РёРєСЃС‚СѓСЂРѕР№. РќРµСЃСѓС‰РёР№ С‚РµСЃС‚ РїРѕРіР»РѕС‰РµРЅРёСЏ Р°РєС†РµРЅС‚РЅРѕРіРѕ GAP #59.
///
/// РЈРґР°Р»СЏРµРјС‹Рµ РїРµСЂРµС‡РёСЃР»РµРЅС‹ СЏРІРЅРѕ СЃ РїСЂРёС‡РёРЅРѕР№ ([`COLLAPSED_ROLES`]) вЂ” С‚РµСЃС‚ РЅРµ В«РїСЂРѕС‰Р°РµС‚В»
/// РёС… РјРѕР»С‡Р°, Р° РґРµРєР»Р°СЂРёСЂСѓРµС‚, РџРћР§Р•РњРЈ РѕРЅРё РЅРµ СЌРјРёС‚РёСЂСѓСЋС‚СЃСЏ (РјР°С‚РµСЂРёР°Р»=С„Р»Р°Рі, СЂРѕР»СЊ РѕС‚
/// С„РѕРЅР°, on-* РІС‹Р±СЂРѕС€РµРЅС‹, С„РѕРЅС‹=РІС…РѕРґС‹, Р°Р»РёР°СЃС‹).
#[test]
fn consumed_roles_diff_is_empty_against_labui_contract() {
    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("С„РёРєСЃС‚СѓСЂР° labui РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    // РџРѕРєСЂС‹С‚РёРµ = СЌРјРёС‚РёСЂСѓРµРјС‹Рµ СЂРѕР»Рё в€Є РєРѕРјРїРѕРЅРµРЅС‚РЅС‹Рµ Р°Р»РёР°СЃС‹ (СЃС‚Р°Р± Р°Р»РёР°СЃРёС‚ РЅРµР№С‚СЂР°Р»СЊРЅС‹Рµ
    // РєРѕРјРїРѕРЅРµРЅС‚РЅС‹Рµ СЂРѕР»Рё С‡РµСЂРµР· var() РЅР° core-СЂРѕР»Рё вЂ” РѕРЅРё РїРѕРєСЂС‹С‚С‹ Р°Р»РёР°СЃРѕРј, РЅРµ СЂРµС†РµРїС‚РѕРј).
    let mut covered: std::collections::HashSet<&str> =
        table.entries().iter().map(|(n, _)| n.as_str()).collect();
    for (alias, _) in &cfg.aliases {
        covered.insert(alias.as_str());
    }

    // РљР°Р¶РґР°СЏ С‚СЂРµР±СѓРµРјР°СЏ (РЅРµ-РєРѕР»Р»Р°РїСЃ) СЂРѕР»СЊ РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РїРѕРєСЂС‹С‚Р° (СЂРµС†РµРїС‚РѕРј РёР»Рё Р°Р»РёР°СЃРѕРј).
    let mut missing = Vec::new();
    for role in LABUI_CONSUMED_ROLES {
        if !covered.contains(role) {
            missing.push(*role);
        }
    }
    assert!(
        missing.is_empty(),
        "diff РќР• РїСѓСЃС‚: С„РёРєСЃС‚СѓСЂР° РЅРµ СЌРјРёС‚РёСЂСѓРµС‚ consumedRoles labui: {missing:?}\n\
         (СѓРґР°Р»СЏРµРјС‹Рµ РїРѕ РєРѕР»Р»Р°РїСЃСѓ РїРµСЂРµС‡РёСЃР»РµРЅС‹ РІ COLLAPSED_ROLES СЃ РїСЂРёС‡РёРЅР°РјРё)"
    );

    // РћР±СЂР°С‚РЅР°СЏ СЃС‚РѕСЂРѕРЅР°: С„РёРєСЃС‚СѓСЂР° РЅРµ СЌРјРёС‚РёС‚ РќР РћР”РќРћР™ РєРѕР»Р»Р°РїСЃ-СЂРѕР»Рё (РёРЅР°С‡Рµ РєРѕР»Р»Р°РїСЃ
    // РЅРµ РёСЃРїРѕР»РЅРµРЅ). РџСЂРµРґРёРєР°С‚ Р’Р«Р’РћР”РРўРЎРЇ РёР· РґРµРєР»Р°СЂР°С†РёР№ COLLAPSED_ROLES вЂ” РІС‚РѕСЂРѕР№,
    // РІСЂСѓС‡РЅСѓСЋ СЃРёРЅС…СЂРѕРЅРёР·РёСЂСѓРµРјС‹Р№ СЃРїРёСЃРѕРє СѓСЃР»РѕРІРёР№ РіРЅРёР» Р±С‹ РјРѕР»С‡Р° (РЅРѕРІС‹Р№ РїР°С‚С‚РµСЂРЅ РІ
    // РґРµРєР»Р°СЂР°С†РёРё Р±РµР· РїСЂР°РІРєРё РїСЂРµРґРёРєР°С‚Р° = С‚РµСЃС‚ РїРµСЂРµСЃС‚Р°С‘С‚ РєСѓСЃР°С‚СЊСЃСЏ).
    for (name, _) in table.entries() {
        if let Some((pattern, why)) = COLLAPSED_ROLES
            .iter()
            .find(|(p, _)| matches_collapsed_pattern(name, p))
        {
            panic!(
                "С„РёРєСЃС‚СѓСЂР° СЌРјРёС‚РёС‚ РєРѕР»Р»Р°РїСЃ-СЂРѕР»СЊ `{name}` (РїР°С‚С‚РµСЂРЅ `{pattern}`: {why}) вЂ” \
                 РєРѕР»Р»Р°РїСЃ РєРѕРЅС‚СЂР°РєС‚Р° РЅР°СЂСѓС€РµРЅ"
            );
        }
    }
    // Р—РЅР°С‡РµРЅС‡РµСЃРєРёР№ РіР°СЂРґ СЃРѕРїРѕСЃС‚Р°РІРёС‚РµР»СЏ (RED-proof РїСЂРѕС‚РёРІ РЅРµРјРѕРіРѕ РїСЂРµРґРёРєР°С‚Р°):
    // РєРѕР»Р»Р°РїСЃ-РёРјРµРЅР° Р»РѕРІСЏС‚СЃСЏ, Р»РµРіРёС‚РёРјРЅР°СЏ FX-СЂРѕР»СЊ `fx-glow-inverted` вЂ” РЅРµС‚
    // (РѕРЅР° РќР• РёРЅРІРµСЂС‚РёСЂРѕРІР°РЅРЅС‹Р№ Р»РµР№Р±Р»/Р±РѕСЂРґРµСЂ).
    let hits = |name: &str| {
        COLLAPSED_ROLES
            .iter()
            .any(|(p, _)| matches_collapsed_pattern(name, p))
    };
    assert!(hits("label-on-accent") && hits("bg-material-thick") && hits("tint-static-dark-4"));
    assert!(!hits("fx-glow-inverted") && !hits("label-danger-primary"));
}

/// Glob-СЃРѕРїРѕСЃС‚Р°РІР»РµРЅРёРµ РїР°С‚С‚РµСЂРЅРѕРІ [`COLLAPSED_ROLES`] (`*` вЂ” Р»СЋР±Р°СЏ РїРѕРґСЃС‚СЂРѕРєР°):
/// СЃРµРіРјРµРЅС‚С‹ РјРµР¶РґСѓ `*` РѕР±СЏР·Р°РЅС‹ РІС…РѕРґРёС‚СЊ РїРѕ РїРѕСЂСЏРґРєСѓ; Р±РµР· РІРµРґСѓС‰РµР№/Р·Р°РјС‹РєР°СЋС‰РµР№ `*`
/// РїРµСЂРІС‹Р№/РїРѕСЃР»РµРґРЅРёР№ СЃРµРіРјРµРЅС‚ Р·Р°СЏРєРѕСЂРµРЅ РЅР° РЅР°С‡Р°Р»Рѕ/РєРѕРЅРµС† РёРјРµРЅРё.
fn matches_collapsed_pattern(name: &str, pattern: &str) -> bool {
    let segments: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;
    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            continue;
        }
        let Some(found) = name[pos..].find(seg) else {
            return false;
        };
        if i == 0 && found != 0 {
            return false; // Р±РµР· РІРµРґСѓС‰РµР№ `*` вЂ” СЏРєРѕСЂСЊ РЅР° РЅР°С‡Р°Р»Рѕ
        }
        pos += found + seg.len();
    }
    // Р‘РµР· Р·Р°РјС‹РєР°СЋС‰РµР№ `*` вЂ” СЏРєРѕСЂСЊ РЅР° РєРѕРЅРµС† РёРјРµРЅРё.
    pattern.ends_with('*') || pos == name.len()
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// РќРµРїСЂРёРєРѕСЃРЅРѕРІРµРЅРЅРѕСЃС‚СЊ РєР»РёРµРЅС‚СЃРєРёС… СЏРєРѕСЂРµР№.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// РСЃС‚РѕС‡РЅРёРє СЃРµРјРµР№СЃС‚РІР° РІС‹Р±РёСЂР°РµС‚ РЅСѓР¶РЅС‹Р№ РєР»РёРµРЅС‚СЃРєРёР№ СЏРєРѕСЂСЊ РґР»СЏ РєРѕРЅС‚РµРєСЃС‚Р°,
/// РЅРѕ РЅРёРєРѕРіРґР° РЅРµ РїРµСЂРµРёРЅС‚РµСЂРїСЂРµС‚РёСЂСѓРµС‚ Рё РЅРµ СЃРјРµС‰Р°РµС‚ РµРіРѕ С„РёР·РёС‡РµСЃРєРѕРµ Р·РЅР°С‡РµРЅРёРµ.
#[test]
fn family_sources_preserve_authored_anchors_in_every_context() {
    let mut cfg = labui_reference();
    for family in &cfg.palette {
        cfg.roles.push((
            format!("probe-family-{}", family.key),
            RoleRecipe::Ladder {
                source: LadderSource::Family(family.key.clone()),
                position: LadderPosition::FillPrimary,
                floor: None,
            },
        ));
    }
    let table = cfg
        .compile_named_role_table()
        .expect("С„РёРєСЃС‚СѓСЂР° РІР°Р»РёРґРЅР°");

    for family in &cfg.palette {
        let role = format!("probe-family-{}", family.key);
        let anchors = &family.anchors;
        let (_, spec) = table
            .entries()
            .iter()
            .find(|(name, _)| name == &role)
            .expect("СЂРѕР»СЊ РµСЃС‚СЊ РІ С„РёРєСЃС‚СѓСЂРµ");
        let RoleSpec::Ladder { tint, .. } = spec else {
            panic!("{role}: РѕР¶РёРґР°Р»СЃСЏ Ladder-СЃРїРµРє, РїРѕР»СѓС‡РµРЅРѕ {spec:?}");
        };
        let modes = [
            ("light", ViewingConditions::srgb(), &anchors.light),
            ("dark", ViewingConditions::dim_surround(), &anchors.dark),
            (
                "light-ic",
                ViewingConditions::srgb_high_contrast(),
                &anchors.light_ic,
            ),
            (
                "dark-ic",
                ViewingConditions::dim_surround_high_contrast(),
                &anchors.dark_ic,
            ),
        ];

        for (mode, vc, authored) in modes {
            let got = crate::spaces::srgb::hex_from_srgb_encoded(tint.for_vc(&vc));
            assert_eq!(
                got, *authored,
                "{role}/{mode}: family source moved authored anchor {authored}"
            );
        }
    }
}

/// A family key is an opaque client ID: consistently renaming the declaration
/// and every reference must compile to the identical physical graph.
#[test]
fn renaming_family_id_and_references_does_not_change_the_compiled_graph() {
    fn rename_source(source: &mut LadderSource, from: &str, to: &str) {
        if let LadderSource::Family(key) = source {
            if key == from {
                *key = to.to_string();
            }
        }
    }

    let original = labui_reference();
    let mut renamed = original.clone();
    renamed
        .palette
        .iter_mut()
        .find(|family| family.key == "red")
        .expect("red fixture family")
        .key = "client-family-42".to_string();

    for (_, recipe) in &mut renamed.roles {
        match recipe {
            RoleRecipe::TextAnchor { hue, .. } => {
                if let Some(source) = hue {
                    rename_source(source, "red", "client-family-42");
                }
            }
            RoleRecipe::Ladder { source, .. }
            | RoleRecipe::Glow { source, .. }
            | RoleRecipe::Material { source, .. } => {
                rename_source(source, "red", "client-family-42");
            }
            RoleRecipe::AlphaAnalog { of, .. } => {
                rename_source(of, "red", "client-family-42");
            }
            RoleRecipe::DjAnchor { .. } | RoleRecipe::DecorativeLc { .. } | RoleRecipe::Zero => {}
        }
    }

    assert_eq!(
        renamed.compile_named_role_table().expect("renamed config"),
        original
            .compile_named_role_table()
            .expect("original config"),
    );
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅР°СЏ СЌРјРёСЃСЃРёСЏ + RED-proof РјСѓС‚Р°С†РёР№ (РїРѕР·РёС†РёСЏ/СЃРµРјРµР№СЃС‚РІРѕ/Р°Р»СЊС„Р° в†’ RED).
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// Р РµР·РѕР»РІ Ladder-СЂРѕР»Рё РЅРµСЃС‘С‚ rgba(С‚РёРЅС‚, О±) + СЃРѕР»РёРґ-РєРѕРјРїРѕР·РёС‚ РЅР° С„РѕРЅРµ СЂРµР·РѕР»РІР°.
/// РўРёРЅС‚ brand-СЂРѕР»Рё РїРѕ СЃРІРµС‚Р»РѕР№ С‚РµРјРµ == СЃРІРµС‚Р»С‹Р№ СЏРєРѕСЂСЊ Р±СЂРµРЅРґР° (СЌРјРёС‚РёС‚СЃСЏ РЅР°РїСЂСЏРјСѓСЋ);
/// РєРѕРјРїРѕР·РёС‚ вЂ” С‚Рѕ, С‡С‚Рѕ СЂРµР°Р»СЊРЅРѕ РїРѕРєР°Р·С‹РІР°РµС‚СЃСЏ РЅР° Р±РµР»РѕРј С„РѕРЅРµ.
#[test]
fn ladder_emits_translucent_with_composite_over_bg() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    let set = resolve_named_set(&bg, &table, &vc)
        .expect("РІР°Р»РёРґРЅР°СЏ ladder-С„РёРєСЃС‚СѓСЂР° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");

    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fill-brand-secondary")
        .unwrap();
    let Resolved::Translucent(r) = res else {
        panic!("fill-brand-secondary: РѕР¶РёРґР°Р»СЃСЏ Translucent, РїРѕР»СѓС‡РµРЅРѕ {res:?}");
    };
    // РўРёРЅС‚ brand light = #007AFF (СЌРјРёС‚РёС‚СЃСЏ РЅР°РїСЂСЏРјСѓСЋ).
    assert_eq!(
        r.tint_hex(),
        "#007AFF",
        "С‚РёРЅС‚ brand-СЂРѕР»Рё (СЃРІРµС‚Р»Р°СЏ С‚РµРјР°)"
    );
    // РђР»СЊС„Р° РїРѕР·РёС†РёРё fill-secondary = @8.
    assert!(
        (r.alpha() - 0.078).abs() < 1e-12,
        "Р°Р»СЊС„Р° fill-secondary @8"
    );
    // РљРѕРјРїРѕР·РёС‚ #007AFF@0.078 РЅР°Рґ #FFFFFF вЂ” С‚Рѕ, С‡С‚Рѕ СЂРµР°Р»СЊРЅРѕ РєСЂР°СЃРёС‚СЃСЏ.
    let want_composite = crate::alpha::composite_hex("#007AFF", 0.078, "#FFFFFF").unwrap();
    assert_eq!(
        r.composite_hex(),
        want_composite,
        "РєРѕРјРїРѕР·РёС‚ РЅР° Р±РµР»РѕРј С„РѕРЅРµ"
    );
    // РљРѕРЅС‚СЂР°СЃС‚ РјРµСЂСЏРµС‚СЃСЏ РЅР° РљРћРњРџРћР—РРўР•, РЅРµ РЅР° С‚РёРЅС‚Рµ: Сѓ РїСЂРѕР·СЂР°С‡РЅРѕР№ Р·Р°Р»РёРІРєРё @8 РЅР°Рґ
    // Р±РµР»С‹Рј РєРѕРјРїРѕР·РёС‚ РїРѕС‡С‚Рё Р±РµР»С‹Р№ вЂ” WCAG Р±Р»РёР·РѕРє Рє 1 Рё Р·Р°РІРµРґРѕРјРѕ РњР•РќР¬РЁР• РєРѕРЅС‚СЂР°СЃС‚Р°
    // СЃРѕР»РёРґРЅРѕРіРѕ С‚РёРЅС‚Р° (#007AFF РЅР° Р±РµР»РѕРј в‰€ 4.0) вЂ” РЅРµС‚Р°РІС‚РѕР»РѕРіРёС‡РЅР°СЏ РїСЂРѕРІРµСЂРєР° С‚РѕРіРѕ,
    // С‡С‚Рѕ Р·Р°РјРµСЂ РёРґС‘С‚ РїРѕ РїСЂР°РІРёР»СЊРЅРѕРјСѓ С†РІРµС‚Сѓ.
    let solid_wcag = crate::spaces::srgb::encoded_srgb_contrast_ratio(
        crate::spaces::srgb::srgb_encoded_from_hex("#007AFF").expect("РІР°Р»РёРґРЅС‹Р№ hex"),
        crate::spaces::srgb::srgb_encoded_from_hex("#FFFFFF").expect("РІР°Р»РёРґРЅС‹Р№ hex"),
    );
    assert!(
        r.composite_wcag() < 1.2 && r.composite_wcag() < solid_wcag / 2.0,
        "WCAG РѕР±СЏР·Р°РЅ РјРµСЂСЏС‚СЊСЃСЏ РїРѕ РєРѕРјРїРѕР·РёС‚Сѓ (РїРѕС‡С‚Рё Р±РµР»РѕРјСѓ), РЅРµ РїРѕ С‚РёРЅС‚Сѓ: composite={}, solid={}",
        r.composite_wcag(),
        solid_wcag
    );
}

/// RED-proof: РїРѕРґРјРµРЅР° РџРћР—РР¦РР Р»РµСЃС‚РЅРёС†С‹ (fill-secondary @8 в†’ label-primary СЃРѕР»РёРґ)
/// РјРµРЅСЏРµС‚ СЌРјРёС‚РёСЂСѓРµРјСѓСЋ Р°Р»СЊС„Сѓ вЂ” РёРЅР°С‡Рµ СЂРµС†РµРїС‚ Р±С‹Р» Р±С‹ СЃР»РµРї Рє РїРѕР·РёС†РёРё.
#[test]
fn ladder_bites_on_position_mutation() {
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Brand,
                position: LadderPosition::LabelPrimary, // СЃРѕР»РёРґ РІРјРµСЃС‚Рѕ @8
                floor: None,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("РІР°Р»РёРґРЅР°СЏ ladder-С„РёРєСЃС‚СѓСЂР° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fill-brand-secondary")
        .unwrap();
    let Resolved::Translucent(r) = res else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Translucent")
    };
    assert!(
        (r.alpha() - 1.0).abs() < 1e-12,
        "RED-proof РїРѕР·РёС†РёРё РїСЂРѕРІР°Р»РµРЅ: Р°Р»СЊС„Р° РЅРµ СЃРјРµРЅРёР»Р°СЃСЊ РЅР° СЃРѕР»РёРґ (1.0), Р° = {}",
        r.alpha()
    );
}

/// RED-proof: РїРѕРґРјРµРЅР° РЎР•РњР•Р™РЎРўР’Рђ РёСЃС‚РѕС‡РЅРёРєР° (dangerв†’red РЅР° successв†’green) РјРµРЅСЏРµС‚
/// СЌРјРёС‚РёСЂСѓРµРјС‹Р№ С‚РёРЅС‚ вЂ” РёРЅР°С‡Рµ СЂРµС†РµРїС‚ Р±С‹Р» Р±С‹ СЃР»РµРї Рє РёСЃС‚РѕС‡РЅРёРєСѓ.
#[test]
fn ladder_bites_on_family_source_mutation() {
    let base = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();
    let base_tint = {
        let set = resolve_named_set(&bg, &base, &vc)
            .expect("РІР°Р»РёРґРЅР°СЏ Р±Р°Р·РѕРІР°СЏ ladder-С„РёРєСЃС‚СѓСЂР° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
        let (_, res) = set
            .iter()
            .find(|(n, _)| n == "fill-danger-primary")
            .unwrap();
        res.translucent().unwrap().tint_hex().to_string()
    };

    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fill-danger-primary" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Family("green".to_string()),
                position: LadderPosition::FillPrimary,
                floor: None,
            };
        }
    }
    let mutated = cfg.compile_named_role_table().unwrap();
    let mutated_tint = {
        let set = resolve_named_set(&bg, &mutated, &vc)
            .expect("РІР°Р»РёРґРЅС‹Р№ family-РјСѓС‚Р°РЅС‚ РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
        let (_, res) = set
            .iter()
            .find(|(n, _)| n == "fill-danger-primary")
            .unwrap();
        res.translucent().unwrap().tint_hex().to_string()
    };
    assert_ne!(
        base_tint, mutated_tint,
        "RED-proof СЃРµРјРµР№СЃС‚РІР° РїСЂРѕРІР°Р»РµРЅ: РїРѕРґРјРµРЅР° dangerв†’green РќР• СЃРјРµРЅРёР»Р° С‚РёРЅС‚ ({base_tint})"
    );
}

/// AlphaAnalog-СЂРµС†РµРїС‚: СЃРѕР»РёРґ-С†РµР»СЊ С„РёРєСЃРёСЂРѕРІР°РЅР°, С‚РёРЅС‚ РІС‹РІРѕРґРёС‚СЃСЏ
/// РєРѕРјРїРѕР·РёС‚-РёРЅРІРµСЂСЃРёРµР№. RED-proof: СЂР°Р·РЅС‹Рµ О± (РѕР±Рµ в‰Ґ О±_min) РґР°СЋС‚ СЂР°Р·РЅС‹Р№ С‚РёРЅС‚;
/// exact gate РґРѕРїСѓСЃРєР°РµС‚ С‚РѕР»СЊРєРѕ РїРѕР±Р°Р№С‚РЅРѕРµ СЂР°РІРµРЅСЃС‚РІРѕ С„РёРЅР°Р»СЊРЅРѕРіРѕ occurrence С†РµР»Рё.
///
/// Р¤РѕРЅ РїРѕРґРѕР±СЂР°РЅ С‚Р°Рє, С‡С‚РѕР±С‹ СЃРѕР»РёРґ Р±С‹Р» СЂР°Р·СЂРµС€РёРј РїСЂРё О± < 1 (РёРЅР°С‡Рµ СЃРѕР»РёРґ РЅР°Рґ Р±РµР»С‹Рј
/// РІС‹СЂРѕР¶РґР°РµС‚СЃСЏ РІ О±_minв‰€1 вЂ” СЌС‚Рѕ С„РёР·РёРєР°, РЅРµ Р±Р°Рі: РїРѕР»РЅРѕСЃС‚СЊСЋ РЅР°СЃС‹С‰РµРЅРЅС‹Р№ СЃРѕР»РёРґ РЅР°Рґ
/// Р±РµР»С‹Рј РІРѕСЃРїСЂРѕРёР·РІРѕРґРёС‚СЃСЏ С‚РѕР»СЊРєРѕ СЃРїР»РѕС€РЅС‹Рј С†РІРµС‚РѕРј).
#[test]
fn alpha_analog_recipe_inverts_and_bites_on_alpha() {
    // РЎРѕР»РёРґ-С†РµР»СЊ = СЃРµСЂРѕРµ СЃРµРјРµР№СЃС‚РІРѕ `#787880` (С‚РѕС‡РЅС‹Р№ РєРµР№СЃ Р¶РёРІС‹С… Figma-РїР°СЂ
    // `alpha.rs`), С„РѕРЅ вЂ” Р±РµР»С‹Р№: РёРЅРІРµСЂСЃРёСЏ СЂР°Р·СЂРµС€РёРјР° РїСЂРё О± < 1 (О±_min в‰€ 0.5), С‚РёРЅС‚
    // РѕСЃРјС‹СЃР»РµРЅРЅРѕ РјРµРЅСЏРµС‚СЃСЏ СЃ О±. (РќР°СЃС‹С‰РµРЅРЅС‹Р№ СЃРѕР»РёРґ СЃ maxed-РєР°РЅР°Р»РѕРј РЅР°Рґ Р±РµР»С‹Рј РґР°Р» Р±С‹
    // О±_min = 1 вЂ” СЌС‚Рѕ С„РёР·РёРєР° РЅР°СЃС‹С‰РµРЅРЅРѕРіРѕ С†РІРµС‚Р°, РЅРµ РіРѕРґРёС‚СЃСЏ РґР»СЏ RED-proof Р°Р»СЊС„С‹.)
    let mut base = labui_reference();
    base.palette.push(PaletteFamily {
        key: "probe".to_string(),
        anchors: ThemeAnchors {
            light: "#787880".to_string(),
            dark: "#787880".to_string(),
            light_ic: "#787880".to_string(),
            dark_ic: "#787880".to_string(),
        },
    });
    let bg = BgInput::solid("#FFFFFF").unwrap();
    let vc = ViewingConditions::srgb();

    let resolve_analog = |alpha: f64| -> (String, f64, String) {
        let mut cfg = base.clone();
        cfg.roles.push((
            "probe-tinted".to_string(),
            RoleRecipe::AlphaAnalog {
                of: LadderSource::Family("probe".to_string()),
                alpha,
            },
        ));
        let table = cfg.compile_named_role_table().unwrap();
        let set = resolve_named_set(&bg, &table, &vc)
            .expect("РІР°Р»РёРґРЅС‹Р№ alpha-analog РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
        let (_, res) = set.iter().find(|(n, _)| n == "probe-tinted").unwrap();
        let r = res.translucent().unwrap();
        (
            r.tint_hex().to_string(),
            r.alpha(),
            r.composite_hex().to_string(),
        )
    };

    let (tint_low, a_low, comp_low) = resolve_analog(0.5);
    let (tint_high, a_high, comp_high) = resolve_analog(0.9);
    // РћР±Рµ О± СЂР°Р·СЂРµС€РёРјС‹ РЅР°Рґ Р±Р»РёР·РєРёРј С„РѕРЅРѕРј в†’ С‚РёРЅС‚ СЂР°Р·Р»РёС‡Р°РµС‚СЃСЏ РїРѕ О± (РєСѓСЃР°РµС‚СЃСЏ).
    assert!(
        tint_low != tint_high || (a_low - a_high).abs() > 1e-6,
        "RED-proof Р°Р»СЊС„С‹ РїСЂРѕРІР°Р»РµРЅ: О±=0.5 Рё О±=0.9 РґР°Р»Рё РѕРґРЅРѕ ({tint_low}@{a_low} vs {tint_high}@{a_high})"
    );
    // Р­РјРёСЃСЃРёРѕРЅРЅС‹Р№ РєРѕРЅС‚СЂР°РєС‚ byte-grid С‚РѕС‡РµРЅ: С„Р°РєС‚РёС‡РµСЃРєРёР№ occurrence РѕР±СЏР·Р°РЅ
    // РІРѕСЃРїСЂРѕРёР·РІРµСЃС‚Рё СЃРѕР»РёРґ-С†РµР»СЊ РїРѕР±Р°Р№С‚РЅРѕ, Р° РЅРµ РїРѕРїР°СЃС‚СЊ РІ СЌРІСЂРёСЃС‚РёС‡РµСЃРєРёР№ LSB-РґРѕРїСѓСЃРє.
    for comp in [&comp_low, &comp_high] {
        assert_eq!(comp, "#787880");
    }
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// РљР»Р°СЃСЃ В«РёРјРµРЅР° Р±РµР· Р·РЅР°С‡РµРЅРёР№В»: Р·РЅР°С‡РµРЅС‡РµСЃРєРёР№ С‚РµСЃС‚ С„РёРєСЃС‚СѓСЂС‹ РїСЂРѕС‚РёРІ СЃС‚Р°Р±Р°.
//
// РљР»Р°СЃСЃ РґРµС„РµРєС‚Р°: СЂРѕР»СЊ РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚ РІ diff-С‚РµСЃС‚Рµ РїРѕ РРњР•РќР, РЅРѕ СЌРјРёС‚РёС‚ РќР• РўРћ
// Р·РЅР°С‡РµРЅРёРµ (РЅР°РїСЂ. РЅРµР№С‚СЂР°Р»СЊРЅС‹Р№ skeleton, РѕС€РёР±РѕС‡РЅРѕ РІР·СЏС‚С‹Р№ РёР· СЃРµРјРµР№СЃС‚РІР° blue).
// Р—РґРµСЃСЊ СЌРјРёСЃСЃРёСЏ rgba(С‚РёРЅС‚, О±) РїСЂРµРґСЃС‚Р°РІРёС‚РµР»СЏ РєР°Р¶РґРѕР№ РіСЂСѓРїРїС‹ СЃРІРµСЂСЏРµС‚СЃСЏ СЃРѕ СЃС‚СЂРѕРєРѕР№
// СЃС‚Р°Р±Р° contract.css РџРћР‘РђР™РўРќРћ (РЅРѕСЂРјР°Р»РёР·РѕРІР°РЅРЅС‹Р№ С„РѕСЂРјР°С‚), РІ light Р dark.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// РќРѕСЂРјР°Р»РёР·РѕРІР°С‚СЊ [`Resolved::Translucent`] РІ РїР°СЂСѓ (rgb-СЃС‚СЂРѕРєР°, О±): rgb СЃРІРµСЂСЏРµС‚СЃСЏ СЃРѕ
/// СЃС‚Р°Р±РѕРј РџРћР‘РђР™РўРћР’Рћ, О± вЂ” С‡РёСЃР»РѕРј СЃ РґРѕРїСѓСЃРєРѕРј (Display-СЃСЂР°РІРЅРµРЅРёРµ f64 С…СЂСѓРїРєРѕ:
/// С…РІРѕСЃС‚ РІРёРґР° 0.07800000000000001 РїРѕСЃР»Рµ Р±СѓРґСѓС‰РµРіРѕ СЂРµС„Р°РєС‚РѕСЂРёРЅРіР° С„РѕСЂРјСѓР»С‹ СѓСЂРѕРЅРёР»
/// Р±С‹ С‚РµСЃС‚ РїРѕ Р¤РћР РњРђРўРЈ, РјР°СЃРєРёСЂСѓСЏ СЃРµРјР°РЅС‚РёРєСѓ).
fn translucent_to_parts(res: &Resolved) -> (String, f64) {
    let r = res.translucent().unwrap_or_else(|| {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Resolved::Translucent, РїРѕР»СѓС‡РµРЅРѕ {res:?}")
    });
    let rgb = crate::spaces::srgb::srgb_encoded_from_hex(r.tint_hex()).unwrap();
    let ch = |v: f64| (v * 255.0).round() as u8;
    (
        format!("rgb({} {} {})", ch(rgb[0]), ch(rgb[1]), ch(rgb[2])),
        r.alpha(),
    )
}

/// Р Р°Р·Р±РёС‚СЊ СЃС‚Р°Р±-Р»РёС‚РµСЂР°Р» `rgb(R G B / A)` / `rgb(R G B)` РЅР° (rgb-СЃС‚СЂРѕРєР°, О±):
/// СЃРѕР»РёРґ Р±РµР· СЃР»СЌС€Р° РЅРµСЃС‘С‚ О± = 1.
fn split_stub_rgba(stub: &str) -> (String, f64) {
    match stub.split_once(" / ") {
        Some((rgb, a)) => (
            format!("{rgb})"),
            a.trim_end_matches(')')
                .parse()
                .expect("О± СЃС‚Р°Р±Р° вЂ” С‡РёСЃР»Рѕ"),
        ),
        None => (stub.to_string(), 1.0),
    }
}

/// РЎРІРµСЂРёС‚СЊ СЌРјРёСЃСЃРёСЋ СЂРѕР»Рё СЃРѕ СЃС‚Р°Р±-Р»РёС‚РµСЂР°Р»РѕРј: rgb РїРѕР±Р°Р№С‚РѕРІРѕ, О± СЃ РґРѕРїСѓСЃРєРѕРј 1e-12
/// (С‚РѕС‚ Р¶Рµ РґРѕРїСѓСЃРє, С‡С‚Рѕ Сѓ СЃРѕСЃРµРґРЅРёС… С‡РёСЃР»РµРЅРЅС‹С… СЃРІРµСЂРѕРє О±).
#[track_caller]
fn assert_matches_stub(role: &str, theme: &str, got: &Resolved, want: &str) {
    let (got_rgb, got_alpha) = translucent_to_parts(got);
    let (want_rgb, want_alpha) = split_stub_rgba(want);
    assert_eq!(
        got_rgb, want_rgb,
        "Р—РќРђР§Р•РќРР• Р РђР—РћРЁР›РћРЎР¬ ({theme}) `{role}`: rgb {got_rgb} != СЃС‚Р°Р± {want_rgb}"
    );
    assert!(
        (got_alpha - want_alpha).abs() < 1e-12,
        "Р—РќРђР§Р•РќРР• Р РђР—РћРЁР›РћРЎР¬ ({theme}) `{role}`: О± {got_alpha} != СЃС‚Р°Р± {want_alpha}"
    );
}

/// Р—РЅР°С‡РµРЅС‡РµСЃРєР°СЏ СЃРІРµСЂРєР° РїСЂРµРґСЃС‚Р°РІРёС‚РµР»РµР№ РіСЂСѓРїРї РїСЂРѕС‚РёРІ СЃС‚Р°Р±Р° labui РІ light Р dark.
/// Р—Р°РєСЂС‹РІР°РµС‚ РєР»Р°СЃСЃ В«РёРјСЏ РµСЃС‚СЊ, Р·РЅР°С‡РµРЅРёРµ РІСЂС‘С‚В»: skeleton = РЅРµР№С‚СЂР°Р»СЊ #787880 СЃ
/// РїРµСЂ-С‚РµРјРЅРѕР№ Р°Р»СЊС„РѕР№, glow-neutral = Р±РµР»С‹Р№ @52, Р°РєС†РµРЅС‚С‹ = РїРµСЂ-С‚РµРјРЅС‹Р№ СЏРєРѕСЂСЊ.
///
/// РСЃРєР»СЋС‡РµРЅС‹ РЅР°РјРµСЂРµРЅРЅРѕ РЅРµРїСЂРµРґСЃС‚Р°РІРёС‚РµР»СЊРЅС‹Рµ `fx-focus-ring-neutral` (dark),
/// `fx-glow-inverted` Рё `fill-neutral`: РёС… РїРµСЂ-С‚РµРјРЅС‹Р№ РЅРµР№С‚СЂР°Р»СЊРЅС‹Р№ РєСЂР°Р№, inverted-СЏРєРѕСЂСЏ Рё
///   Р·Р°РґРѕРєСѓРјРµРЅС‚РёСЂРѕРІР°РЅРЅС‹Рµ gap-Рё (РїРµСЂ-С‚РµРјРЅС‹Р№ РЅРµР№С‚СЂР°Р»СЊРЅС‹Р№ РєСЂР°Р№ / inverted-СЏРєРѕСЂСЏ /
///   PROVISIONAL-Р»РёС‚РµСЂР°Р» РЅРµ РІС‹РІРѕРґСЏС‚СЃСЏ РёР· С‚СЂРѕР№РєРё neutral.anchors).
#[test]
fn representative_roles_match_stub_values_light_and_dark() {
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg_light = BgInput::solid("#FFFFFF").unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();

    // (СЂРѕР»СЊ, СЃС‚Р°Р±-light, СЃС‚Р°Р±-dark). Р—РЅР°С‡РµРЅРёСЏ вЂ” РёР· contract.css (2026-07-02).
    let cases: &[(&str, &str, &str)] = &[
        // Р Р°С‚РёС„РёРєР°С†РёСЏ ch5c (M1): `label-<family>-<level>` Р±РѕР»СЊС€Рµ РќР• Ladder@72/52/32
        // (С‚РёРЅС‚ СЃРµРјСЊРё РїРѕРґ Р°Р»СЊС„РѕР№ вЂ” 40/40 РЅР°СЂСѓС€РµРЅРёР№ РѕРґРЅРѕСѓСЂРѕРІРЅРµРІРѕСЃС‚Рё), Р° С†РІРµС‚РЅРѕР№
        // TextAnchor вЂ” РґРµСЂР¶РёС‚ Lc-РєРѕРЅС‚СЂР°РєС‚ СѓСЂРѕРІРЅСЏ РІ С‡РёСЃС‚РѕРј РѕС‚С‚РµРЅРєРµ СЃРµРјСЊРё Рё
        // СЂРµР·РѕР»РІРёС‚СЃСЏ РІ РЎРћР›РР” (`Resolved::Color`), РЅРµ РІ Translucent. Р•РіРѕ СЌРјРёСЃСЃРёСЏ Рё
        // РѕРґРЅРѕСѓСЂРѕРІРЅРµРІРѕСЃС‚СЊ РїСЂРѕРІРµСЂСЏСЋС‚СЃСЏ РјРѕРґСѓР»РµРј `src/one_levelness_tests.rs`. Р—РґРµСЃСЊ
        // РѕСЃС‚Р°С‘С‚СЃСЏ РїСЂРµРґСЃС‚Р°РІРёС‚РµР»СЊ РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅРѕР№ РЎР•РњР•Р™РќРћР™ Р·Р°Р»РёРІРєРё (С‚РѕС‚ Р¶Рµ
        // СЃРµРјРµР№РЅС‹Р№ С‚РёРЅС‚ РїРѕРґ Р°Р»СЊС„РѕР№ СЂР°РјРїС‹ вЂ” РєР»Р°СЃСЃ В«РёРјСЏ РµСЃС‚СЊ, Р·РЅР°С‡РµРЅРёРµ С‚РёРЅС‚Р° РІСЂС‘С‚В»).
        (
            "fill-danger-secondary",
            "rgb(255 59 48 / 0.078)",
            "rgb(255 58 58 / 0.078)",
        ),
        (
            "fill-brand-primary",
            "rgb(0 122 255 / 0.122)",
            "rgb(74 143 255 / 0.122)",
        ),
        (
            "border-success-base",
            "rgb(52 199 89 / 0.2)",
            "rgb(48 209 88 / 0.2)",
        ),
        // fx-glow-brand РІС‹РІРµРґРµРЅ РёР· Р·РЅР°С‡РµРЅС‡РµСЃРєРѕР№ СЃРІРµСЂРєРё СЃРѕ СЃС‚Р°Р±РѕРј: СЃ 2026-07-03
        // СЌС‚Рѕ kind glow (screen-СЃР»РѕРё + СЂРµС€С‘РЅРЅР°СЏ О±), Р° РЅРµ Ladder@52 вЂ” СЃС‚Р°Р±-СЃС‚СЂРѕРєР°
        // rgba Р±РѕР»СЊС€Рµ РЅРµ СЏРІР»СЏРµС‚СЃСЏ РµРіРѕ РєРѕРЅС‚СЂР°РєС‚РѕРј. РќРѕРІР°СЏ СЌРјРёСЃСЃРёСЏ Р·Р°РєСЂРµРїР»РµРЅР°
        // РѕС‚РґРµР»СЊРЅС‹Рј С‚РµСЃС‚РѕРј `glow_roles_resolve_screen_layers`.
        // РљСЂР°СЏ РЅРµР№С‚СЂР°Р»Рё РїРµСЂ-С‚РµРјРЅС‹Рµ: РєРѕРЅС‚СѓСЂ (edge) Рё РёРЅРІРµСЂС‚ вЂ” РёР· СЃС‚Р°Р±Р° РґРѕСЃР»РѕРІРЅРѕ.
        ("fx-focus-ring-neutral", "rgb(16 16 18)", "rgb(246 248 250)"),
        (
            "fx-glow-inverted",
            "rgb(176 176 185 / 0.522)",
            "rgb(60 60 67 / 0.522)",
        ),
        // РњРёРіСЂРёСЂРѕРІР°РЅРЅС‹Рµ РЅР° Р»РµСЃС‚РЅРёС†Сѓ РЅРµР№С‚СЂР°Р»СЊРЅС‹Рµ Р·Р°Р»РёРІРєРё/РіСЂР°РЅРёС†С‹: rgba(mid, О±)
        // СЃ РїРµСЂ-С‚РµРјРЅС‹РјРё РїР°СЂР°РјРё вЂ” РґРѕСЃР»РѕРІРЅРѕ Р·РЅР°С‡РµРЅРёСЏ СЃС‚Р°Р±Р° (РёСЃС‚РёРЅР° РјРёРіСЂР°С†РёРё).
        (
            "fill-primary",
            "rgb(120 120 128 / 0.2)",
            "rgb(120 120 128 / 0.361)",
        ),
        (
            "fill-secondary",
            "rgb(120 120 128 / 0.161)",
            "rgb(120 120 128 / 0.322)",
        ),
        (
            "fill-tertiary",
            "rgb(120 120 128 / 0.122)",
            "rgb(120 120 128 / 0.239)",
        ),
        (
            "fill-quaternary",
            "rgb(120 120 128 / 0.078)",
            "rgb(120 120 128 / 0.161)",
        ),
        (
            "border-base",
            "rgb(120 120 128 / 0.161)",
            "rgb(120 120 128 / 0.2)",
        ),
        (
            "border-soft",
            "rgb(120 120 128 / 0.078)",
            "rgb(120 120 128 / 0.122)",
        ),
        // РќРµР№С‚СЂР°Р»СЊРЅС‹Рµ: skeleton highlight #787880 @4; base вЂ” Р°Р»РёР°СЃ
        // fill-quaternary (РЅР°СЃР»РµРґРѕРІР°РЅРёРµ СЃР»Р°Р±С‹С… Р·Р°Р»РёРІРѕРє: С‡РµС‚РІРµСЂРёС‡РЅР°СЏ Р·Р°Р»РёРІРєР° =
        // disabled-СѓСЂРѕРІРµРЅСЊ, СЃРєРµР»РµС‚РѕРЅ = Р±СѓРґСѓС‰Р°СЏ С„РѕСЂРјР°), СЌРјРёСЃСЃРёСЋ Р°Р»РёР°СЃР°
        // РїСЂРѕРІРµСЂСЏРµС‚ РіСЂР°РЅРёС†Р°. glow-neutral Р±РµР»С‹Р№ @52.
        (
            "fx-skeleton-highlight",
            "rgb(120 120 128 / 0.039)",
            "rgb(120 120 128 / 0.039)",
        ),
        // РўРµРЅРё: С‚С‘РјРЅС‹Р№ СЏРєРѕСЂСЊ РЅРµР№С‚СЂР°Р»Рё (#101012) РІ РћР‘Р•РРҐ С‚РµРјР°С…, РїРѕР»СѓРїСЂРѕР·СЂР°С‡РЅРѕСЃС‚СЊ by design вЂ”
        // СЃРѕР»РёРґ РЅР°Рґ РєР°СЂС‚РёРЅРєРѕР№/СЃС‚РµРєР»РѕРј Р·Р°РєСЂС‹РІР°Р» Р±С‹ РєРѕРЅС‚РµРЅС‚ РїСЏС‚РЅРѕРј.
        (
            "fx-shadow-minor",
            "rgb(16 16 18 / 0.012)",
            "rgb(16 16 18 / 0.02)",
        ),
        (
            "fx-shadow-ambient",
            "rgb(16 16 18 / 0.02)",
            "rgb(16 16 18 / 0.039)",
        ),
        (
            "fx-shadow-penumbra",
            "rgb(16 16 18 / 0.039)",
            "rgb(16 16 18 / 0.122)",
        ),
        (
            "fx-shadow-major",
            "rgb(16 16 18 / 0.122)",
            "rgb(16 16 18 / 0.2)",
        ),
        // fx-glow-neutral РІС‹РІРµРґРµРЅ РёР· СЃРІРµСЂРєРё СЃРѕ СЃС‚Р°Р±РѕРј: kind glow СЃ 2026-07-03
        // (СЃРј. РєРѕРјРјРµРЅС‚Р°СЂРёР№ Сѓ fx-glow-brand РІС‹С€Рµ Рё С‚РµСЃС‚ glow_roles_resolve_screen_layers).
    ];

    for (role, want_light, want_dark) in cases {
        let set_l = resolve_named_set(&bg_light, &table, &ViewingConditions::srgb()).expect(
            "РІР°Р»РёРґРЅР°СЏ СЃРІРµС‚Р»Р°СЏ fixture РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ",
        );
        let set_d = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround())
            .expect("РІР°Р»РёРґРЅР°СЏ С‚С‘РјРЅР°СЏ fixture РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
        let got_l = &set_l.iter().find(|(n, _)| n == role).unwrap().1;
        let got_d = &set_d.iter().find(|(n, _)| n == role).unwrap().1;
        assert_matches_stub(role, "light", got_l, want_light);
        assert_matches_stub(role, "dark", got_d, want_dark);
    }
}

/// RED-proof Р·РЅР°С‡РµРЅС‡РµСЃРєРѕРіРѕ С‚РµСЃС‚Р°: РјСѓС‚Р°С†РёСЏ РћР”РќРћР™ Р°Р»СЊС„С‹ (skeleton-highlight
/// @4 в†’ NeutralFillPrimary @36 РЅР° С‚С‘РјРЅРѕР№) СЂРѕРЅСЏРµС‚ СЃРІРµСЂРєСѓ вЂ” С‚РµСЃС‚ РєСѓСЃР°РµС‚СЃСЏ,
/// РЅРµ green-from-birth.
#[test]
fn value_test_bites_on_alpha_mutation() {
    let mut cfg = labui_reference();
    for (name, recipe) in &mut cfg.roles {
        if name == "fx-skeleton-highlight" {
            *recipe = RoleRecipe::Ladder {
                source: LadderSource::Neutral(crate::config::NeutralPick::Mid),
                position: LadderPosition::NeutralFillPrimary,
                floor: None,
            };
        }
    }
    let table = cfg.compile_named_role_table().unwrap();
    let bg_dark = BgInput::solid("#101012").unwrap();
    let set = resolve_named_set(&bg_dark, &table, &ViewingConditions::dim_surround())
        .expect("РІР°Р»РёРґРЅР°СЏ alpha-mutation fixture РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
    let (got_rgb, got_alpha) = translucent_to_parts(
        &set.iter()
            .find(|(n, _)| n == "fx-skeleton-highlight")
            .unwrap()
            .1,
    );
    assert_eq!(
        got_rgb, "rgb(120 120 128)",
        "РјСѓС‚Р°С†РёСЏ РґРІРёРіР°РµС‚ РўРћР›Р¬РљРћ Р°Р»СЊС„Сѓ"
    );
    assert!(
        (got_alpha - 0.039).abs() > 1e-9,
        "RED-proof Р·РЅР°С‡РµРЅС‡РµСЃРєРѕРіРѕ С‚РµСЃС‚Р° РїСЂРѕРІР°Р»РµРЅ: РјСѓС‚Р°С†РёСЏ Р°Р»СЊС„С‹ РќР• СЃРґРІРёРЅСѓР»Р° СЌРјРёСЃСЃРёСЋ"
    );
}

/// Р”СѓР±Р»РёРєР°С‚С‹ РєР»СЋС‡РµР№ РІСЃРµС… СЃР»РѕРІР°СЂРµР№ РѕС‚РІРµСЂРіР°СЋС‚СЃСЏ (РїРѕРІС‚РѕСЂ РёРјРµРЅРё = РЅРµРѕРґРЅРѕР·РЅР°С‡РЅС‹Р№
/// lookup), РІРєР»СЋС‡Р°СЏ Р°Р»РёР°СЃ, Р·Р°С‚РµРЅСЏСЋС‰РёР№ СЂРѕР»СЊ.
#[test]
fn validator_rejects_duplicate_dictionary_keys() {
    let mut c = labui_reference();
    c.roles.push(c.roles[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "roles",
            ..
        })
    ));

    // C5.1: РёРјСЏ С‚РµРјС‹ вЂ” РєР»СЋС‡ РєР»РёРµРЅС‚СЃРєРѕРіРѕ СЃР»РѕРІР°СЂСЏ; РґСѓР±Р»РёРєР°С‚ РґРµР»Р°Р» Р±С‹ lookup
    // РЅРµРѕРґРЅРѕР·РЅР°С‡РЅС‹Рј (first-wins С‚РёС…Рѕ С…РѕСЂРѕРЅРёС‚ РІС‚РѕСЂСѓСЋ РґРµРєР»Р°СЂР°С†РёСЋ).
    let mut c = labui_reference();
    c.themes.entries.push(c.themes.entries[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "themes",
            ..
        })
    ));

    let mut c = labui_reference();
    c.palette.push(c.palette[0].clone());
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "palette",
            ..
        })
    ));

    let mut c = labui_reference();
    let role_name = c.roles[0].0.clone();
    c.aliases.push((role_name, c.roles[1].0.clone()));
    assert!(matches!(
        c.validate(),
        Err(ConfigError::DuplicateKey {
            dictionary: "rolesв€Єaliases",
            ..
        })
    ));
}

/// РРјРµРЅР° РєРѕРЅС„РёРіР° вЂ” РЅРµ РІРµСЃСЊ namespace СЌРјРёСЃСЃРёРё: Glow РґРѕР±Р°РІР»СЏРµС‚ `-core/-alpha`,
/// Material вЂ” `-01/-02`. Р РѕР»СЊ РёР»Рё Р°Р»РёР°СЃ СЃ С‚Р°РєРёРј РёРјРµРЅРµРј СЂР°РЅСЊС€Рµ РїСЂРѕС…РѕРґРёР»Рё
/// preflight, Р° JSON-РїСЂРѕРµРєС†РёСЏ РјРѕР»С‡Р° Р·Р°РїРёСЃС‹РІР°Р»Р° РѕРґРёРЅ `--lab-*` РґРІР°Р¶РґС‹; РїРѕСЃР»РµРґРЅРёР№
/// РїРёСЃР°С‚РµР»СЊ РјРµРЅСЏР» С‚РёРї Р·РЅР°С‡РµРЅРёСЏ (РЅР°РїСЂРёРјРµСЂ, alpha-С‡РёСЃР»Рѕ РїСЂРµРІСЂР°С‰Р°Р»РѕСЃСЊ РІ С†РІРµС‚).
#[test]
fn validator_rejects_role_and_alias_collisions_with_emitted_satellites() {
    let assert_collision = |cfg: ThemeConfig, expected_stem: &str| {
        let expected = format!("--lab-{expected_stem}");
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::DuplicateKey {
                    dictionary: "reserved CSS namespace",
                    key,
                }) if key == expected
            ),
            "РєРѕР»Р»РёР·РёСЏ СЌРјРёС‚РёСЂСѓРµРјРѕРіРѕ РєР»СЋС‡Р° {expected} РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚Р°"
        );
    };

    // Р РµР°Р»СЊРЅС‹Р№ РєР»Р°СЃСЃ СЂРµРіСЂРµСЃСЃРёРё: СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёР№ glow РІР»Р°РґРµРµС‚ РґРІСѓРјСЏ СЃР°С‚РµР»Р»РёС‚Р°РјРё.
    for suffix in ["-core", "-alpha"] {
        let colliding = format!("fx-glow-brand{suffix}");

        let mut role_cfg = labui_reference();
        let ordinary_recipe = role_cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("С„РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ label-primary")
            .1
            .clone();
        role_cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(role_cfg, &colliding);

        let mut alias_cfg = labui_reference();
        alias_cfg
            .aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(alias_cfg, &colliding);
    }

    // РўРѕС‚ Р¶Рµ Р·Р°РєРѕРЅ РѕР±СЏР·Р°РЅ Р·Р°РєСЂС‹РІР°С‚СЊ РІС‚РѕСЂРѕР№ РјРЅРѕРіРѕРєР»СЋС‡РµРІРѕР№ outcome, Р° РЅРµ С‚РѕР»СЊРєРѕ
    // РєРѕРЅРєСЂРµС‚РЅС‹Р№ РЅР°Р№РґРµРЅРЅС‹Р№ СЃСѓС„С„РёРєСЃ glow.
    for suffix in ["-01", "-02"] {
        let colliding = format!("probe-material{suffix}");

        let mut role_cfg = labui_reference();
        role_cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        let ordinary_recipe = role_cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("С„РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ label-primary")
            .1
            .clone();
        role_cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(role_cfg, &colliding);

        let mut alias_cfg = labui_reference();
        alias_cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        alias_cfg
            .aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(alias_cfg, &colliding);
    }

    // РђР»РёР°СЃ РјРЅРѕРіРѕРєР»СЋС‡РµРІРѕР№ С†РµР»Рё СЃР°Рј СЃС‚Р°РЅРѕРІРёС‚СЃСЏ РІР»Р°РґРµР»СЊС†РµРј РїРѕР»РЅРѕРіРѕ shape. Р­С‚Рѕ
    // РѕС‚РґРµР»СЊРЅР°СЏ РІРµС‚РІСЊ: РїСЂРѕРІРµСЂРєР° С‚РѕР»СЊРєРѕ СЂРµС†РµРїС‚РѕРІ СЂРѕР»РµР№ РїСЂРѕРїСѓСЃС‚РёР»Р° Р±С‹ РµС‘.
    for suffix in ["-core", "-alpha"] {
        let owner = "probe-glow-alias";
        let colliding = format!("{owner}{suffix}");
        let mut cfg = labui_reference();
        cfg.aliases
            .push((owner.to_string(), "fx-glow-brand".to_string()));
        let ordinary_recipe = cfg
            .roles
            .iter()
            .find(|(name, _)| name == "label-primary")
            .expect("С„РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ label-primary")
            .1
            .clone();
        cfg.roles.push((colliding.clone(), ordinary_recipe));
        assert_collision(cfg, &colliding);
    }

    for suffix in ["-01", "-02"] {
        let owner = "probe-material-alias";
        let colliding = format!("{owner}{suffix}");
        let mut cfg = labui_reference();
        cfg.roles.push((
            "probe-material".to_string(),
            neutral_material(10.0, Floor::AaText),
        ));
        cfg.aliases
            .push((owner.to_string(), "probe-material".to_string()));
        cfg.aliases
            .push((colliding.clone(), "label-primary".to_string()));
        assert_collision(cfg, &colliding);
    }
}

/// `Zero` РЅРµ СЌРјРёС‚РёС‚ Р·РЅР°С‡РµРЅРёРµ, РЅРѕ РµРіРѕ РєР»РёРµРЅС‚СЃРєРѕРµ РёРјСЏ РІСЃС‘ СЂР°РІРЅРѕ Р·Р°РЅСЏС‚Рѕ: РёРЅР°С‡Рµ
/// СЃР°С‚РµР»Р»РёС‚ РґСЂСѓРіРѕР№ СЂРѕР»Рё РјРѕРі Р±С‹ Р·Р°РїРёСЃР°С‚СЊ С†РІРµС‚ РІ `cssVar` С‚РѕРєРµРЅР° СЃ `kind: "none"`.
/// Р—Р°РєРѕРЅ РѕРґРёРЅР°РєРѕРІ РґР»СЏ СЏРІРЅРѕР№ zero-СЂРѕР»Рё Рё Р°Р»РёР°СЃР° РЅР° РЅРµС‘, Р° С‚Р°РєР¶Рµ РґР»СЏ РєР°Р¶РґРѕРіРѕ
/// РјРЅРѕРіРѕРєР»СЋС‡РµРІРѕРіРѕ shape, РёР·РІРµСЃС‚РЅРѕРіРѕ core.
#[test]
fn validator_reserves_zero_role_and_alias_primary_names() {
    let assert_collision = |cfg: ThemeConfig, expected_stem: &str| {
        let expected = format!("--lab-{expected_stem}");
        assert!(
            matches!(
                cfg.validate(),
                Err(ConfigError::DuplicateKey {
                    dictionary: "reserved CSS namespace",
                    key,
                }) if key == expected
            ),
            "zero-С‚РѕРєРµРЅ РѕР±СЏР·Р°РЅ Р·Р°С‰РёС‰Р°С‚СЊ Р·Р°СЂРµР·РµСЂРІРёСЂРѕРІР°РЅРЅС‹Р№ CSS key {expected}"
        );
    };

    let glow_recipe = labui_reference()
        .roles
        .into_iter()
        .find(|(name, _)| name == "fx-glow-brand")
        .expect("С„РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ fx-glow-brand")
        .1;

    for (owner, recipe, suffixes) in [
        ("probe-glow", glow_recipe, &["-core", "-alpha"][..]),
        (
            "probe-material",
            neutral_material(10.0, Floor::AaText),
            &["-01", "-02"][..],
        ),
    ] {
        for suffix in suffixes {
            let zero_name = format!("{owner}{suffix}");

            let mut role_cfg = labui_reference();
            role_cfg.roles.push((owner.to_string(), recipe.clone()));
            role_cfg.roles.push((zero_name.clone(), RoleRecipe::Zero));
            assert_collision(role_cfg, &zero_name);

            let mut alias_cfg = labui_reference();
            alias_cfg.roles.push((owner.to_string(), recipe.clone()));
            alias_cfg
                .aliases
                .push((zero_name.clone(), "none".to_string()));
            assert_collision(alias_cfg, &zero_name);
        }
    }
}

/// Р“Р°СЂРґ РЅРµ РґРѕР»Р¶РµРЅ РїСЂРµРІСЂР°С‰Р°С‚СЊСЃСЏ РІ Р·Р°РїСЂРµС‚ РїРѕС…РѕР¶РёС… РїСЂРµС„РёРєСЃРѕРІ: СЂРµР·РµСЂРІРёСЂСѓСЋС‚СЃСЏ СЂРѕРІРЅРѕ
/// С„Р°РєС‚РёС‡РµСЃРєРё СЌРјРёС‚РёСЂСѓРµРјС‹Рµ РёРјРµРЅР°, Р° РЅРµ РІСЃРµ СЃС‚СЂРѕРєРё, РЅР°С‡РёРЅР°СЋС‰РёРµСЃСЏ СЃ РёРјРµРЅРё СЂРѕР»Рё.
#[test]
fn emitted_namespace_allows_non_colliding_near_misses() {
    let mut cfg = labui_reference();
    cfg.aliases.push((
        "fx-glow-brand-alpha-extra".to_string(),
        "label-primary".to_string(),
    ));
    cfg.roles.push((
        "probe-material".to_string(),
        neutral_material(10.0, Floor::AaText),
    ));
    cfg.aliases
        .push(("probe-material-03".to_string(), "label-primary".to_string()));

    assert_eq!(cfg.validate(), Ok(()));
}

#[test]
fn compiled_output_binding_set_is_exact_ordered_and_alias_aware() {
    let source = labui_reference();
    let ordinary = source
        .roles
        .iter()
        .find(|(name, _)| name == "label-primary")
        .expect("fixture carries an ordinary role")
        .1
        .clone();
    let glow = source
        .roles
        .iter()
        .find(|(name, _)| name == "fx-glow-brand")
        .expect("fixture carries a Glow role")
        .1
        .clone();

    let mut cfg = source;
    cfg.roles = vec![
        ("plain".to_string(), ordinary),
        ("pulse".to_string(), glow),
        ("glass".to_string(), neutral_material(10.0, Floor::AaText)),
        ("empty".to_string(), RoleRecipe::Zero),
    ];
    cfg.aliases = vec![
        ("pulse-alias".to_string(), "pulse".to_string()),
        ("glass-alias".to_string(), "glass".to_string()),
        ("empty-alias".to_string(), "empty".to_string()),
    ];

    let table = cfg
        .compile_named_role_table()
        .expect("fixture is a valid executable contract");
    assert_eq!(
        table.output_bindings().keys(),
        [
            "--lab-plain",
            "--lab-pulse",
            "--lab-pulse-core",
            "--lab-pulse-alpha",
            "--lab-glass",
            "--lab-glass-01",
            "--lab-glass-02",
            "--lab-empty",
            "--lab-pulse-alias",
            "--lab-pulse-alias-core",
            "--lab-pulse-alias-alpha",
            "--lab-glass-alias",
            "--lab-glass-alias-01",
            "--lab-glass-alias-02",
            "--lab-empty-alias",
        ]
    );
}

#[test]
fn output_binding_compile_errors_preserve_the_config_error_contract() {
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::InvalidName {
            kind: OutputBindingNameKind::Role,
            value: "bad key".to_string(),
        }),
        ConfigError::InvalidName {
            field: "roles.bad key".to_string(),
            value: "bad key".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::InvalidName {
            kind: OutputBindingNameKind::Alias,
            value: "bad alias".to_string(),
        }),
        ConfigError::InvalidName {
            field: "aliases.bad alias".to_string(),
            value: "bad alias".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::UnknownAliasTarget {
            alias: "shortcut".to_string(),
            target: "missing".to_string(),
        }),
        ConfigError::UnknownRole {
            referenced_by: "aliases.shortcut".to_string(),
            role: "missing".to_string(),
        }
    );
    assert_eq!(
        map_output_binding_error(OutputBindingCompileError::DuplicateBinding {
            key: "--lab-pulse-core".to_string(),
        }),
        ConfigError::DuplicateKey {
            dictionary: "reserved CSS namespace",
            key: "--lab-pulse-core".to_string(),
        }
    );
}

/// РќРµРєРѕРЅРµС‡РЅС‹Рµ Р·РЅР°С‡РµРЅРёСЏ СЂСѓС‡РµРє (в€ћ/NaN) РѕС‚РІРµСЂРіР°СЋС‚СЃСЏ Рё open-СЃРІРµСЂС…Сѓ РїСЂРµРґРµР»Р°РјРё.
#[test]
fn validator_rejects_non_finite_handles() {
    for bad in [f64::INFINITY, f64::NAN] {
        // Р—РѕРЅРґ-СЂРµС†РµРїС‚: dj-СЏРєРѕСЂРµР№ РІ С„РёРєСЃС‚СѓСЂРµ Р±РѕР»СЊС€Рµ РЅРµС‚ (РЅРµР№С‚СЂР°Р»СЊРЅС‹Рµ
        // Р·Р°Р»РёРІРєРё/РіСЂР°РЅРёС†С‹ СѓРµС…Р°Р»Рё РЅР° Р»РµСЃС‚РЅРёС†Сѓ), Р° РїСЂРµРґРµР» СЂСѓС‡РєРё вЂ” СЃРІРѕР№СЃС‚РІРѕ РњР•РќР®.
        let mut c = labui_reference();
        c.roles.push((
            "probe-dj".to_string(),
            RoleRecipe::DjAnchor {
                light: bad,
                dark: 5.0,
            },
        ));
        assert!(
            matches!(c.validate(), Err(ConfigError::OutOfBounds { .. })),
            "dj={bad} РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚"
        );
    }
}

/// РћС€РёР±РєРё СЃСЃС‹Р»РѕРє СЂР°Р·Р»РёС‡РёРјС‹ РїРѕ РІРёРґСѓ: СЂРѕР»СЊ Рё СЃРµРјРµР№СЃС‚РІРѕ вЂ” СЂР°Р·РЅС‹Рµ РІР°СЂРёР°РЅС‚С‹.
#[test]
fn validator_reference_errors_are_distinguishable() {
    let mut c = labui_reference();
    c.roles.push((
        "probe-bad-family".to_string(),
        RoleRecipe::Ladder {
            source: LadderSource::Family("nonexistent".to_string()),
            position: LadderPosition::LabelPrimary,
            floor: None,
        },
    ));
    assert!(matches!(
        c.validate(),
        Err(ConfigError::UnknownFamily { .. })
    ));

    let mut c = labui_reference();
    c.aliases
        .push(("probe-alias".to_string(), "nonexistent-role".to_string()));
    assert!(matches!(c.validate(), Err(ConfigError::UnknownRole { .. })));
}

/// IC-РЅР°СЃР»РµРґРѕРІР°РЅРёРµ Р°Р»СЊС„ Р·Р°РєСЂРµРїР»РµРЅРѕ: РїРѕР·РёС†РёСЏ РѕС‚РґР°С‘С‚ Р°Р»СЊС„Сѓ Р±Р°Р·РѕРІРѕР№ С‚РµРјС‹ Рё РІ
/// IC-СЂРµР¶РёРјРµ (IC РјРµРЅСЏРµС‚ С‚РёРЅС‚, РЅРµ РїСЂРѕР·СЂР°С‡РЅРѕСЃС‚СЊ вЂ” СЃС‚Р°Р± Р±РµР· ic-СЃРєРѕСѓРїРѕРІ).
#[test]
fn ic_inherits_base_theme_alpha() {
    use crate::spaces::vc::ViewingConditions;
    let pos = crate::ladder::LadderPosition::SkeletonBase;
    let light = ViewingConditions::srgb();
    let dark = ViewingConditions::dim_surround();
    let light_ic = ViewingConditions::srgb_high_contrast();
    let dark_ic = ViewingConditions::dim_surround_high_contrast();
    assert_eq!(pos.alpha_for_vc(&light), pos.alpha_for_vc(&light_ic));
    assert_eq!(pos.alpha_for_vc(&dark), pos.alpha_for_vc(&dark_ic));
    // РџРµСЂ-С‚РµРјРЅР°СЏ РїР°СЂР° СЂРµР°Р»СЊРЅРѕ СЂР°Р·Р»РёС‡Р°РµС‚СЃСЏ (skeleton-base @8/@12).
    assert!((pos.alpha_for_vc(&light) - pos.alpha_for_vc(&dark)).abs() > 1e-6);
}

/// РђР»РёР°СЃС‹ РїРµСЂРµРЅРѕСЃСЏС‚СЃСЏ РІ СЃРєРѕРјРїРёР»РёСЂРѕРІР°РЅРЅСѓСЋ С‚Р°Р±Р»РёС†Сѓ вЂ” Р±РµР· РїРµСЂРµРЅРѕСЃР° Р°Р»РёР°СЃРЅС‹Рµ СЂРѕР»Рё
/// РєРѕРЅС‚СЂР°РєС‚Р° С‚РµСЂСЏР»РёСЃСЊ Р±С‹ РїСЂРё СЌРјРёСЃСЃРёРё.
#[test]
fn compiled_table_carries_aliases() {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("С„РёРєСЃС‚СѓСЂР° РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    let aliases = table.aliases();
    assert!(
        !aliases.is_empty(),
        "С„РёРєСЃС‚СѓСЂР° РЅРµСЃС‘С‚ Р°Р»РёР°СЃС‹"
    );
    assert!(
        aliases
            .iter()
            .any(|(a, t)| a == "fill-neutral-tinted" && t == "fill-primary"),
        "Р°Р»РёР°СЃ fill-neutral-tintedв†’fill-primary РѕР±СЏР·Р°РЅ РїРµСЂРµР¶РёС‚СЊ РєРѕРјРїРёР»СЏС†РёСЋ"
    );
}

/// РџСЂСЏРјРѕР№ NamedRoleTable-РєРѕРЅСЃС‚СЂСѓРєС‚РѕСЂ РЅРµ РґРѕРїСѓСЃРєР°РµС‚ РїСЂР°РІРґРѕРїРѕРґРѕР±РЅС‹Р№ РјСѓСЃРѕСЂ:
/// РЅРµРІР°Р»РёРґРЅР°СЏ О± РѕС‚РІРµСЂРіР°РµС‚СЃСЏ РґРѕ СЃРѕР·РґР°РЅРёСЏ executable-С‚Р°Р±Р»РёС†С‹.
#[test]
fn translucent_resolve_rejects_out_of_domain_spec() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};
    let tint =
        crate::ladder::LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("РІР°Р»РёРґРЅС‹Р№ С‚РёРЅС‚");
    for bad_alpha in [f64::NAN, 0.0, 1.5] {
        let result = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::Ladder {
                    tint,
                    alpha_light: bad_alpha,
                    alpha_dark: bad_alpha,
                    floor: None,
                },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "О±={bad_alpha} РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚Р° РґРѕ resolve"
        );
    }
    // РњСѓСЃРѕСЂРЅС‹Р№ quad РѕС‚РІРµСЂРіР°РµС‚СЃСЏ РєРѕРЅСЃС‚СЂСѓРєС‚РѕСЂРѕРј С‚РёРЅС‚Р° СЃ РёРјРµРЅРµРј СЂРµР¶РёРјР°.
    assert_eq!(
        crate::ladder::LadderTint::new([[2.0, 0.5, 0.5]; 4]).unwrap_err(),
        "light"
    );
}

/// The public semantic constructor is an executable boundary of its own: it
/// must reject a role name before it can become an invalid CSS custom property.
#[test]
fn named_role_table_rejects_invalid_role_name_before_materialisation() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    for invalid in ["", "bad key", "Upper", "under_score", "СЂРѕР»СЊ"] {
        let result = NamedRoleTable::new(
            vec![(invalid.to_string(), RoleSpec::Zero)],
            Vec::new(),
            RoleChroma::Neutral,
        );

        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "invalid role name {invalid:?} must fail before an output manifest exists: {result:?}"
        );
    }
}

/// Aliases reserve public output names too, so the direct constructor applies
/// the same name law to them instead of delegating it to `ThemeConfig`.
#[test]
fn named_role_table_rejects_invalid_alias_name_before_materialisation() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let result = NamedRoleTable::new(
        vec![("valid".to_string(), RoleSpec::Zero)],
        vec![("bad alias".to_string(), "valid".to_string())],
        RoleChroma::Neutral,
    );

    assert!(
        matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
        "invalid alias name must fail before an output manifest exists: {result:?}"
    );
}

#[test]
fn named_role_table_rejects_unknown_alias_target_at_its_own_boundary() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let error = NamedRoleTable::new(
        vec![("valid".to_string(), RoleSpec::Zero)],
        vec![("shortcut".to_string(), "missing".to_string())],
        RoleChroma::Neutral,
    )
    .expect_err("unknown alias target must fail before a table exists");

    assert_eq!(
        error,
        crate::solve::SolveFailure::InvalidInput(
            "alias \"shortcut\" targets unknown executable role \"missing\"".to_string()
        )
    );
}

/// Direct semantic construction must run the same exact namespace collision
/// gate as configuration compilation, including recipe satellites and aliases.
#[test]
fn named_role_table_rejects_role_and_alias_satellite_collisions() {
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let compiled = labui_reference()
        .compile_named_role_table()
        .expect("reference contract compiles");
    let glow = compiled
        .entries()
        .iter()
        .find_map(|(_, spec)| matches!(spec, RoleSpec::Glow { .. }).then_some(*spec))
        .expect("reference contract carries a Glow recipe");

    for result in [
        NamedRoleTable::new(
            vec![
                ("pulse".to_string(), glow),
                ("pulse-core".to_string(), RoleSpec::Zero),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        ),
        NamedRoleTable::new(
            vec![
                ("pulse".to_string(), glow),
                ("plain".to_string(), RoleSpec::Zero),
            ],
            vec![("pulse-alpha".to_string(), "plain".to_string())],
            RoleChroma::Neutral,
        ),
    ] {
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "colliding output shapes must fail before a table exists: {result:?}"
        );
    }
}

/// Р“СЂР°РЅРёС†С‹ О± AlphaAnalog: СЂРѕРІРЅРѕ 1.0 РІР°Р»РёРґРЅР°, 1.0+Оµ вЂ” РЅРµС‚ (RED-proof РіСЂР°РЅРё).
#[test]
fn alpha_analog_boundary_is_exact() {
    let mut c = labui_reference();
    c.roles.push((
        "probe-alpha-boundary".to_string(),
        RoleRecipe::AlphaAnalog {
            of: LadderSource::Brand,
            alpha: 1.0,
        },
    ));
    assert!(c.validate().is_ok(), "О±=1.0 Р»РµРіР°Р»СЊРЅР°");
    if let Some((_, RoleRecipe::AlphaAnalog { alpha, .. })) = c
        .roles
        .iter_mut()
        .find(|(n, _)| n == "probe-alpha-boundary")
    {
        *alpha = 1.0 + 1e-9;
    }
    assert!(
        matches!(c.validate(), Err(ConfigError::OutOfBounds { .. })),
        "О± С‡СѓС‚СЊ РІС‹С€Рµ 1 РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚Р°"
    );
}

/// Edge/Inverted Р±РµР· СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰РµРіРѕ РїРѕР»СЏ РєРѕРЅС„РёРіР° вЂ” С‡РµСЃС‚РЅР°СЏ РѕС€РёР±РєР°, РЅРµ РІС‹РґСѓРјРєР°.
#[test]
fn missing_neutral_quads_are_rejected() {
    let mut c = labui_reference();
    c.neutral.edge = None;
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::MissingNeutralAnchors {
            field: "neutral.edge",
            ..
        })
    ));
    let mut c = labui_reference();
    c.neutral.inverted = None;
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::MissingNeutralAnchors {
            field: "neutral.inverted",
            ..
        })
    ));
}

/// `validate()` вЂ” РїРѕР»РЅС‹Р№ preflight РџРћ РџРћРЎРўР РћР•РќРР® (РєРѕРјРїРёР»СЏС†РёСЏ СЃ РѕС‚Р±СЂРѕС€РµРЅРЅС‹Рј
/// СЂРµР·СѓР»СЊС‚Р°С‚РѕРј): РґР»СЏ Р»СЋР±РѕРіРѕ РєРѕРЅС„РёРіР° validate Рё compile РґР°СЋС‚ РѕРґРёРЅР°РєРѕРІС‹Р№ РёСЃС…РѕРґ
/// Рё Р±Р°Р№С‚-РІ-Р±Р°Р№С‚ РѕРґРёРЅР°РєРѕРІСѓСЋ РѕС€РёР±РєСѓ. РљРѕСЂРїСѓСЃ вЂ” РґРµСЂРёРІР°С†РёРѕРЅРЅС‹Рµ РѕС€РёР±РєРё, РєРѕС‚РѕСЂС‹Рµ
/// СЃС‚СЂСѓРєС‚СѓСЂРЅР°СЏ С„Р°Р·Р° РЅРµ РІРёРґРёС‚ (РёРЅР°С‡Рµ `Ok` preflight-Р° Р±С‹Р» Р±С‹ Р»РѕР¶РЅРѕРїРѕР»РѕР¶РёС‚РµР»СЊРЅС‹Рј).
#[test]
fn validate_is_a_complete_preflight() {
    let ok = labui_reference();
    assert!(
        ok.validate().is_ok(),
        "РєР°РЅРѕРЅРёС‡РµСЃРєРёР№ РєРѕРЅС„РёРі РїСЂРѕС…РѕРґРёС‚ preflight"
    );
    assert!(ok.compile_named_role_table().is_ok());

    let assert_parity = |c: &ThemeConfig, want: &str| {
        let v = c
            .validate()
            .expect_err("validate РѕР±СЏР·Р°РЅ РїР°РґР°С‚СЊ");
        let k = c
            .compile_named_role_table()
            .expect_err("compile РѕР±СЏР·Р°РЅ РїР°РґР°С‚СЊ");
        assert_eq!(
            format!("{v:?}"),
            format!("{k:?}"),
            "validate Рё compile СЂР°Р·РѕС€Р»РёСЃСЊ вЂ” РїРѕР»РЅРѕС‚Р° preflight РЅР°СЂСѓС€РµРЅР°"
        );
        let got = format!("{v:?}");
        assert!(
            got.contains(want),
            "Р¶РґР°Р»Рё {want}, РїРѕР»СѓС‡РµРЅРѕ {got}"
        );
    };

    // РўРѕС‡РЅС‹Р№ СЃРµСЂС‹Р№ Р±РµР· override Р·Р°РєРѕРЅРµРЅ, РїРѕРєР° РІСЃРµ material-РёСЃС‚РѕС‡РЅРёРєРё Р°С…СЂРѕРјР°С‚РёС‡РЅС‹;
    // validate Рё compile РѕР±СЏР·Р°РЅС‹ РѕРґРёРЅР°РєРѕРІРѕ РїСЂРёРЅСЏС‚СЊ С‚Р°РєРѕР№ РєРѕРЅС‚СЂР°РєС‚.
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string();
    assert!(c.validate().is_ok());
    assert!(c.compile_named_role_table().is_ok());

    // Р¦РІРµС‚РЅРѕР№ РёСЃС‚РѕС‡РЅРёРє РЅРµР»СЊР·СЏ РѕС‚Р»РѕР¶РёС‚СЊ РІРЅСѓС‚СЂСЊ neutral-policy: preflight РѕР±СЏР·Р°РЅ
    // РѕС‚РІРµСЂРіРЅСѓС‚СЊ РєРѕРЅС„Р»РёРєС‚ РґРѕ РїРµСЂРІРѕРіРѕ runtime-resolve.
    for (name, recipe) in &mut c.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Material {
                source: LadderSource::Brand,
                tone_light: 12.0,
                tone_dark: 12.0,
                floor: Floor::AaUi,
            };
        }
    }
    assert!(matches!(
        c.validate(),
        Err(ConfigError::IncompatibleRolePolicy { ref role, .. })
            if role == "fill-brand-secondary"
    ));
    assert!(matches!(
        c.compile_named_role_table(),
        Err(ConfigError::IncompatibleRolePolicy { ref role, .. })
            if role == "fill-brand-secondary"
    ));
    assert_eq!(
        c.validate().unwrap_err().to_string(),
        format!(
            "material `fill-brand-secondary`: {}",
            RoleSpec::INCOMPATIBLE_CHROMA_REASON
        )
    );

    // Edge-СЂРѕР»СЊ Р±РµР· С‡РµС‚РІС‘СЂРєРё edge вЂ” РґРµСЂРёРІР°С†РёРѕРЅРЅР°СЏ РѕС€РёР±РєР° РєСЂР°СЏ РЅРµР№С‚СЂР°Р»Рё.
    let mut c = labui_reference();
    c.neutral.edge = None;
    assert_parity(&c, "MissingNeutralAnchors");

    // Р‘РёС‚С‹Р№ hex РІ Р—РђР”РђРќРќРћР™, РЅРѕ РЅРёРєРµРј РЅРµ РёСЃРїРѕР»СЊР·СѓРµРјРѕР№ С‡РµС‚РІС‘СЂРєРµ edge:
    // Р·Р°РґРµРєР»Р°СЂРёСЂРѕРІР°РЅРЅС‹Рµ РґР°РЅРЅС‹Рµ РІР°Р»РёРґРёСЂСѓСЋС‚СЃСЏ РґР°Р¶Рµ Р±РµР· СЃСЃС‹Р»Р°СЋС‰РёС…СЃСЏ СЂРѕР»РµР№ вЂ”
    // РјС‘СЂС‚РІС‹Р№ Р±РёС‚С‹Р№ hex РЅРµ РґРѕР»Р¶РµРЅ Р¶РґР°С‚СЊ РїРµСЂРІСѓСЋ СЃСЃС‹Р»РєСѓ, С‡С‚РѕР±С‹ РІСЃРїР»С‹С‚СЊ.
    let mut c = labui_reference();
    c.roles.retain(|(_, r)| {
        !matches!(
            r,
            RoleRecipe::Ladder {
                source: LadderSource::Neutral(NeutralPick::Edge),
                ..
            } | RoleRecipe::AlphaAnalog {
                of: LadderSource::Neutral(NeutralPick::Edge),
                ..
            }
        )
    });
    let kept: std::collections::BTreeSet<&str> = c.roles.iter().map(|(n, _)| n.as_str()).collect();
    c.aliases
        .retain(|(_, target)| kept.contains(target.as_str()));
    c.neutral.edge = Some(crate::ladder::ThemeAnchors {
        light: "РЅРµ-hex".to_string(),
        dark: "#F6F8FA".to_string(),
        light_ic: "#101012".to_string(),
        dark_ic: "#F6F8FA".to_string(),
    });
    assert_parity(&c, "InvalidHex");
}

/// `RoleSpec` РїСѓР±Р»РёС‡РµРЅ: alpha-analog-СЃРїРµРєР° СЃ РЅРµРґРѕРјРµРЅРЅРѕР№ О±, СЃРѕР±СЂР°РЅРЅР°СЏ РІ РѕР±С…РѕРґ
/// РІР°Р»РёРґР°С‚РѕСЂР° РєРѕРЅС„РёРіР°, РѕС‚РІРµСЂРіР°РµС‚СЃСЏ РґРѕ СЃРѕР·РґР°РЅРёСЏ executable-С‚Р°Р±Р»РёС†С‹. РќРµРґРѕРјРµРЅРЅС‹Р№ РЎРћР›РР” РїРѕ
/// РїРѕСЃС‚СЂРѕРµРЅРёСЋ РЅРµРІРѕР·РјРѕР¶РµРЅ ([`crate::ladder::LadderTint::new`] РІР°Р»РёРґРёСЂСѓРµС‚ РґРѕРјРµРЅ
/// РєРІР°РґР°) вЂ” РіР°СЂРґ РїРѕ СЃРѕР»РёРґСѓ РѕСЃС‚Р°С‘С‚СЃСЏ РіР»СѓР±РёРЅРЅРѕР№ Р·Р°С‰РёС‚РѕР№.
#[test]
fn alpha_analog_spec_bypassing_validator_is_rejected() {
    use crate::ladder::LadderTint;
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec};

    let tint = LadderTint::new([[0.5, 0.5, 0.5]; 4]).expect("РІР°Р»РёРґРЅС‹Р№ РєРІР°Рґ");
    for alpha in [1.0 + 1e-9, 0.0, -0.5, f64::NAN, f64::INFINITY] {
        let result = NamedRoleTable::new(
            vec![(
                "probe".to_string(),
                RoleSpec::AlphaAnalog { of: tint, alpha },
            )],
            vec![],
            RoleChroma::Neutral,
        );
        assert!(
            matches!(result, Err(crate::solve::SolveFailure::InvalidInput(_))),
            "О±={alpha}: РѕР¶РёРґР°Р»СЃСЏ РѕС‚РєР°Р· РєРѕРЅСЃС‚СЂСѓРєС‚РѕСЂР°, РїРѕР»СѓС‡РµРЅРѕ {result:?}"
        );
    }
}

fn assert_achromatic_hex(hex: &str, context: &str) {
    let [red, green, blue] = crate::srgb8::hex_bytes(hex).expect("canonical emitted hex");
    assert_eq!(
        red, green,
        "{context}: invented red/green direction in {hex}"
    );
    assert_eq!(
        green, blue,
        "{context}: invented green/blue direction in {hex}"
    );
}

fn assert_chromatic_hex(hex: &str, context: &str) {
    let [red, green, blue] = crate::srgb8::hex_bytes(hex).expect("canonical emitted hex");
    assert!(
        red != green || green != blue,
        "{context}: one-byte chromatic direction was discarded in {hex}"
    );
}

/// Exact-gray sources carry neutral identity without an override; the nearest
/// off-axis byte still carries its authored direction.
#[test]
fn achromatic_hue_sources_are_handled_honestly() {
    let mut c = labui_reference();
    c.neutral.tint.hue_override_deg = None;
    c.neutral.anchors.dark = "#101010".to_string();
    let neutral_table = c
        .compile_named_role_table()
        .expect("exact-gray neutral source compiles without an override");
    assert_eq!(neutral_table.chroma(), RoleChroma::Neutral);

    c.neutral.anchors.dark = "#101011".to_string();
    let chromatic_table = c
        .compile_named_role_table()
        .expect("nearest chromatic neutral source retains its direction");
    assert!(matches!(chromatic_table.chroma(), RoleChroma::Curve { .. }));

    let mut c = labui_reference();
    // РЎРµСЂС‹Р№ Р±СЂРµРЅРґ: РІСЃРµ С‡РµС‚С‹СЂРµ СЂРµР¶РёРјР° Р°С…СЂРѕРјР°С‚РёС‡РЅС‹.
    c.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#808080".to_string(),
        dark: "#808080".to_string(),
        light_ic: "#808080".to_string(),
        dark_ic: "#808080".to_string(),
    };
    let table = c
        .compile_named_role_table()
        .expect("СЃРµСЂС‹Р№ Р±СЂРµРЅРґ Р»РµРіР°Р»РµРЅ");
    let set = crate::semantic::resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &crate::spaces::vc::ViewingConditions::srgb(),
    )
    .expect("РІР°Р»РёРґРЅР°СЏ Р°С…СЂРѕРјР°С‚РёС‡РµСЃРєР°СЏ brand-С„РёРєСЃС‚СѓСЂР° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");

    let (_, label) = set
        .iter()
        .find(|(name, _)| name == "label-brand-primary")
        .expect("С†РІРµС‚РЅР°СЏ brand-СЂРѕР»СЊ РµСЃС‚СЊ");
    let Resolved::Color { solved, .. } = label else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Color");
    };
    assert_achromatic_hex(solved.hex(), "TextAnchor from achromatic Brand");

    // Exact-gray Brand must not affect an unrelated client-owned family source.
    // The role name below is opaque fixture data and carries no Core semantics.
    let (_, r) = set
        .iter()
        .find(|(n, _)| n == "fill-danger-primary")
        .expect("СЂРѕР»СЊ РµСЃС‚СЊ");
    let Resolved::Translucent(r) = r else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Translucent");
    };
    assert_eq!(
        r.tint_hex(),
        "#FF3B30",
        "РЅРµР·Р°РІРёСЃРёРјС‹Р№ family-РёСЃС‚РѕС‡РЅРёРє РѕР±СЏР·Р°РЅ СЃРѕС…СЂР°РЅРёС‚СЊ РєР»РёРµРЅС‚СЃРєРёР№ СЏРєРѕСЂСЊ"
    );
}

/// `NeutralPick` вЂ” РґР°РЅРЅС‹Рµ, Р° РЅРµ Р·Р°РїСЂРѕСЃ РЅР° РѕР±С‰РёР№ РїРѕРґС‚РѕРЅ С‚Р°Р±Р»РёС†С‹: РІС‹Р±СЂР°РЅРЅС‹Р№ С‚РѕС‡РЅС‹Р№
/// СЃРµСЂС‹Р№ СЏРєРѕСЂСЊ РѕСЃС‚Р°С‘С‚СЃСЏ Р°С…СЂРѕРјР°С‚РёС‡РµСЃРєРёРј РїСЂРё С†РІРµС‚РЅРѕР№ policy РґСЂСѓРіРёС… РЅРµР№С‚СЂР°Р»РµР№.
#[test]
fn material_uses_the_selected_neutral_source_before_chroma_classification() {
    let mut config = labui_reference();
    config.neutral.tint.hue_override_deg = None;
    assert!(matches!(
        config.compile_named_role_table().unwrap().chroma(),
        RoleChroma::Curve { .. }
    ));
    for (name, recipe) in &mut config.roles {
        if name == "fill-brand-secondary" {
            *recipe = RoleRecipe::Material {
                source: LadderSource::Neutral(NeutralPick::Light),
                tone_light: 12.0,
                tone_dark: 12.0,
                floor: Floor::AaUi,
            };
        }
    }

    let table = config.compile_named_role_table().unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .expect("exact-gray material source must resolve under a chromatic table policy");
    let (_, Resolved::Material(material)) = set
        .iter()
        .find(|(name, _)| name == "fill-brand-secondary")
        .expect("material role exists")
    else {
        panic!("fill-brand-secondary must resolve to Material");
    };
    assert_achromatic_hex(material.tint_hex(), "selected Neutral(Light) Material");
    assert_achromatic_hex(material.base_hex(), "selected Neutral(Light) Material");
}

/// Transitional characterization only: these paths are removed with the closed
/// recipe menu. The durable law lives in `Srgb8`, `SourceHuePlan` and the generic
/// curve tests: exact emitted gray has no hue; the nearest off-axis byte does.
#[test]
fn every_hue_consuming_path_preserves_achromatic_source_identity() {
    let viewing_conditions = [
        ViewingConditions::srgb(),
        ViewingConditions::dim_surround(),
        ViewingConditions::srgb_high_contrast(),
        ViewingConditions::dim_surround_high_contrast(),
    ];

    for vc in viewing_conditions {
        let mut config = labui_reference();
        config.brand.anchors = crate::ladder::ThemeAnchors {
            light: "#808080".to_string(),
            dark: "#808080".to_string(),
            light_ic: "#808080".to_string(),
            dark_ic: "#808080".to_string(),
        };
        for (name, recipe) in &mut config.roles {
            match name.as_str() {
                "label-brand-secondary" => {
                    *recipe = RoleRecipe::TextAnchor {
                        fraction: 0.5,
                        floor: Floor::AaUi,
                        hue: Some(LadderSource::Brand),
                    };
                }
                "fill-brand-secondary" => {
                    *recipe = RoleRecipe::Material {
                        source: LadderSource::Brand,
                        tone_light: 12.0,
                        tone_dark: 12.0,
                        floor: Floor::AaUi,
                    };
                }
                _ => {}
            }
        }

        let table = config
            .compile_named_role_table()
            .expect("achromatic Brand recipes compile");
        let background = if vc.is_dark_theme() {
            BgInput::solid("#101010").unwrap()
        } else {
            BgInput::solid("#FFFFFF").unwrap()
        };
        let set = resolve_named_set(&background, &table, &vc)
            .expect("achromatic source-derived recipes resolve");

        for role in ["label-brand-primary", "label-brand-secondary"] {
            let (_, Resolved::Color { solved, .. }) = set
                .iter()
                .find(|(name, _)| name == role)
                .unwrap_or_else(|| panic!("missing {role}"))
            else {
                panic!("{role} must resolve to Color");
            };
            assert_achromatic_hex(solved.hex(), role);
        }

        let (_, Resolved::Material(material)) = set
            .iter()
            .find(|(name, _)| name == "fill-brand-secondary")
            .expect("material role exists")
        else {
            panic!("fill-brand-secondary must resolve to Material");
        };
        assert_achromatic_hex(material.tint_hex(), "Material tint");
        assert_achromatic_hex(material.base_hex(), "Material base");
    }
}

#[test]
fn nearest_chromatic_source_survives_every_current_source_consuming_path() {
    let mut config = labui_reference();
    config.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#808081".to_string(),
        dark: "#808081".to_string(),
        light_ic: "#808081".to_string(),
        dark_ic: "#808081".to_string(),
    };
    for (name, recipe) in &mut config.roles {
        match name.as_str() {
            "label-brand-secondary" => {
                *recipe = RoleRecipe::TextAnchor {
                    fraction: 0.5,
                    floor: Floor::AaUi,
                    hue: Some(LadderSource::Brand),
                };
            }
            "fill-brand-secondary" => {
                *recipe = RoleRecipe::Material {
                    source: LadderSource::Brand,
                    tone_light: 12.0,
                    tone_dark: 12.0,
                    floor: Floor::AaUi,
                };
            }
            _ => {}
        }
    }

    let table = config.compile_named_role_table().unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();

    for role in ["label-brand-primary", "label-brand-secondary"] {
        let (_, Resolved::Color { solved, .. }) = set
            .iter()
            .find(|(name, _)| name == role)
            .unwrap_or_else(|| panic!("missing {role}"))
        else {
            panic!("{role} must resolve to Color");
        };
        assert_chromatic_hex(solved.hex(), role);
    }

    let (_, Resolved::Material(material)) = set
        .iter()
        .find(|(name, _)| name == "fill-brand-secondary")
        .expect("material role exists")
    else {
        panic!("fill-brand-secondary must resolve to Material");
    };
    assert_chromatic_hex(material.base_hex(), "Material");
}

#[test]
fn achromatic_solid_floor_never_invents_hue() {
    let mut floor_config = labui_reference();
    floor_config.brand.anchors = crate::ladder::ThemeAnchors {
        light: "#E0E0E0".to_string(),
        dark: "#E0E0E0".to_string(),
        light_ic: "#E0E0E0".to_string(),
        dark_ic: "#E0E0E0".to_string(),
    };
    let floor_table = floor_config.compile_named_role_table().unwrap();
    let floor_set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &floor_table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let (_, Resolved::Translucent(border)) = floor_set
        .iter()
        .find(|(name, _)| name == "border-brand-strong")
        .expect("solid border exists")
    else {
        panic!("border-brand-strong must resolve to Translucent");
    };
    assert!(
        border.floor_coerced(),
        "fixture must execute the floor-shift branch"
    );
    assert_achromatic_hex(border.tint_hex(), "solid floor shift");
}

#[test]
fn adjacent_foreign_source_cannot_change_an_independent_solve() {
    use crate::ladder::LadderTint;
    use crate::semantic::{NamedRoleTable, RoleChroma, RoleSpec, TextAnchor};

    let tint = |hex: &str| {
        let encoded = crate::spaces::srgb::srgb_encoded_from_hex(hex).unwrap();
        LadderTint::new([encoded; 4]).unwrap()
    };
    let senior_anchor = TextAnchor::new(0.9, Floor::AaText)
        .unwrap()
        .with_hue(tint("#FF3B30"));
    let junior_anchor = TextAnchor::new(0.8, Floor::AaText)
        .unwrap()
        .with_hue(tint("#808080"));
    let table = NamedRoleTable::new(
        vec![
            ("senior".to_string(), RoleSpec::Anchor(senior_anchor)),
            ("junior".to_string(), RoleSpec::Anchor(junior_anchor)),
        ],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect("two source identities are a valid table");

    let background = BgInput::solid("#6F6F6F").unwrap();
    let vc = ViewingConditions::srgb();
    let set = resolve_named_set(&background, &table, &vc).unwrap();
    let (_, junior) = set
        .iter()
        .find(|(name, _)| name == "junior")
        .expect("junior exists");
    let Resolved::Color { solved, .. } = junior else {
        panic!("junior must resolve to Color");
    };
    assert_achromatic_hex(solved.hex(), "independent achromatic source");

    let isolated = NamedRoleTable::new(
        vec![("junior".to_string(), RoleSpec::Anchor(junior_anchor))],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap();
    let isolated_set = resolve_named_set(&background, &isolated, &vc).unwrap();
    let Resolved::Color {
        solved: isolated_solved,
        ..
    } = &isolated_set[0].1
    else {
        panic!("isolated junior must resolve to Color");
    };
    assert_eq!(solved.hex(), isolated_solved.hex());
    assert_eq!(
        junior, &isolated_set[0].1,
        "an unrelated adjacent source must not change value or provenance"
    );
}

#[test]
fn all_achromatic_material_is_lawful_under_a_neutral_table_policy() {
    use crate::ladder::LadderTint;
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};

    let gray = crate::spaces::srgb::srgb_encoded_from_hex("#808080").unwrap();
    let table = NamedRoleTable::new(
        vec![(
            "material".to_string(),
            RoleSpec::Material {
                hue: Some(LadderTint::new([gray; 4]).unwrap()),
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaUi,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect("all-achromatic source needs no chromatic table policy");
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let Resolved::Material(material) = &set[0].1 else {
        panic!("material must resolve");
    };
    assert_achromatic_hex(material.tint_hex(), "neutral-policy Material");
}

/// РљРћРњРџРћР—РР¦РРћРќРќР«Р™ РєРѕРЅС‚СЂР°РєС‚ FX-СЃС‚РµРєР° С‚РµРЅРµР№.
///
/// РџСЂРµР¶РЅРёР№ РєРѕРЅС‚СЂР°РєС‚ РґРµСЂР¶Р°Р» С‚РѕР»СЊРєРѕ РїРµСЂ-С‚РѕРєРµРЅРЅС‹Р№ РїРѕСЂСЏРґРѕРє (|Lc| РєР°Р¶РґРѕР№ СЃС‚СѓРїРµРЅРё
/// СЃР°РјР° РїРѕ СЃРµР±Рµ). Р—Р°РєРѕРЅ РІР»Р°РґРµР»СЊС†Р° СЃРёР»СЊРЅРµРµ: С‚РѕРєРµРЅС‹ РќРђРЎР›РђРР’РђР®РўРЎРЇ (minor РїРѕРґ
/// ambient РїРѕРґ penumbra РїРѕРґ major), Рё РїСЂРѕРіСЂРµСЃСЃРёРІРЅС‹Рј РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РЎРЈРњРњРђР РќР«Р™
/// СЌС„С„РµРєС‚ composited-СЃС‚РµРєР°. Р—РґРµСЃСЊ СЃС‚РµРє РєРѕРјРїРѕРЅСѓРµС‚СЃСЏ С‡РµСЃС‚РЅРѕР№ Р°Р»СЊС„Р°-РєРѕРјРїРѕР·РёС†РёРµР№
/// (`alpha::composite_over_encoded`, С‚РѕС‚ Р¶Рµ РѕРїРµСЂР°С‚РѕСЂ, С‡С‚Рѕ Сѓ Р±СЂР°СѓР·РµСЂР°) СЃР»РѕР№ Р·Р°
/// СЃР»РѕРµРј РЅР°Рґ СЃРІРµС‚Р»С‹Рј С„РѕРЅРѕРј РїР°СЃРїРѕСЂС‚Р°, Рё РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ:
///   (1) РєР°Р¶РґС‹Р№ СЃР»РѕР№ РјРµРЅСЏРµС‚ РїРёРєСЃРµР»Рё: state_k в‰  state_{k-1} РЅР° 8-Р±РёС‚РЅРѕР№ СЃРµС‚РєРµ
///       (РєР»Р°СЃСЃ `composite_distinct`);
///   (2) СЂР°Р·Р»РёС‡РёРјРѕСЃС‚СЊ СЃС‚РµРєР° РѕС‚ С„РѕРЅР° СЃС‚СЂРѕРіРѕ СЂР°СЃС‚С‘С‚: |О”J'|(state_k, bg)
///       РІРѕР·СЂР°СЃС‚Р°РµС‚ РїРѕ k вЂ” РїСЂРѕРіСЂРµСЃСЃРёСЏ РёРјРµРЅРЅРѕ РљРћРњРџРћР—РР¦РР, РЅРµ РѕС‚РґРµР»СЊРЅС‹С… СЃС‚СѓРїРµРЅРµР№.
///
/// РўС‘РјРЅР°СЏ С‚РµРјР° РЅР°РјРµСЂРµРЅРЅРѕ РЅРµ РІ СЌС‚РѕРј С‚РµСЃС‚Рµ: elevation С‚С‘РјРЅРѕР№ С‚РµРјС‹ вЂ” С‚РѕРЅР°Р»СЊРЅР°СЏ
/// Р»РµСЃС‚РЅРёС†Р° С„РѕРЅРѕРІ (bg-tone-*, dj-anchor РєРѕРЅС‚СЂР°РєС‚С‹ СЌС‚РѕРіРѕ Р¶Рµ РїРѕРµР·РґР°), С‚РµРЅСЊ РЅР°
/// С‚С‘РјРЅРѕРј РІС‹СЂРѕР¶РґР°РµС‚СЃСЏ С„РёР·РёС‡РµСЃРєРё (С‚РёРЅС‚ в‰€ С„РѕРЅ вЂ” РєР»Р°СЃСЃ, РїСЂРёР·РЅР°РЅРЅС‹Р№ ladder.rs);
/// glow-СЃС‚РµРєР° РЅРµ СЃСѓС‰РµСЃС‚РІСѓРµС‚ (fx-glow-* вЂ” РѕРґРёРЅРѕС‡РЅС‹Рµ РїРѕР·РёС†РёРё @52).
#[test]
fn fx_shadow_stack_composition_is_strictly_progressive_on_light() {
    use crate::alpha::composite_over_encoded;
    #[allow(deprecated)] // Solver curve uses deprecated LcsColor per F-01 design
    use crate::lcs::LcsColor;
    use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};

    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("С„РёРєСЃС‚СѓСЂР° labui РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    let vc = ViewingConditions::srgb();
    let bg_hex = "#FFFFFF"; // СЃРІРµС‚Р»С‹Р№ СЏРєРѕСЂСЊ РїР°СЃРїРѕСЂС‚Р° вЂ” С„РѕРЅ СЂРµР·РѕР»РІР° СЃРІРµС‚Р»РѕР№ С‚РµРјС‹
    let bg = BgInput::solid(bg_hex).unwrap();
    let set = resolve_named_set(&bg, &table, &vc)
        .expect("РІР°Р»РёРґРЅС‹Р№ shadow-stack РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");

    let stack = [
        "fx-shadow-minor",
        "fx-shadow-ambient",
        "fx-shadow-penumbra",
        "fx-shadow-major",
    ];
    #[allow(deprecated)] // Solver curve uses deprecated LcsColor per F-01 design
    let bg_jp = LcsColor::from_hex_with_vc(bg_hex, &vc).unwrap().jp();
    let mut state = srgb_encoded_from_hex(bg_hex).unwrap();
    let mut prev_delta = 0.0_f64;
    for name in stack {
        let (_, resolved) = set
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("{name} РѕС‚СЃСѓС‚СЃС‚РІСѓРµС‚ РІ РЅР°Р±РѕСЂРµ"));
        let t = resolved
            .translucent()
            .unwrap_or_else(|| panic!("{name} РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Translucent"));
        let tint = srgb_encoded_from_hex(t.tint_hex()).unwrap();
        let prev_hex = hex_from_srgb_encoded(state);
        state = composite_over_encoded(tint, t.alpha(), state)
            .expect("СЌРјРёС‚РёСЂРѕРІР°РЅРЅС‹Рµ tint/alpha Рё РїСЂРµРґС‹РґСѓС‰РёР№ РєРѕРјРїРѕР·РёС‚ Р»РµР¶Р°С‚ РІ РґРѕРјРµРЅРµ");
        let state_hex = hex_from_srgb_encoded(state);
        // (1) СЃР»РѕР№ РјРµРЅСЏРµС‚ РїРёРєСЃРµР»Рё РїРѕРІРµСЂС… СѓР¶Рµ РЅР°СЃР»РѕС‘РЅРЅРѕРіРѕ СЃС‚РµРєР°.
        assert_ne!(
            state_hex, prev_hex,
            "{name}: РЅР°СЃР»РѕРµРЅРёРµ СЃР»РѕСЏ РЅРµ РёР·РјРµРЅРёР»Рѕ РєРѕРјРїРѕР·РёС‚ ({state_hex}) вЂ” РІС‹СЂРѕР¶РґРµРЅРЅР°СЏ СЃС‚СѓРїРµРЅСЊ СЃС‚РµРєР°"
        );
        // (2) СЃСѓРјРјР°СЂРЅР°СЏ СЂР°Р·Р»РёС‡РёРјРѕСЃС‚СЊ СЃС‚РµРєР° РѕС‚ С„РѕРЅР° СЃС‚СЂРѕРіРѕ СЂР°СЃС‚С‘С‚.
        #[allow(deprecated)] // Solver curve uses deprecated LcsColor per F-01 design
        let jp = LcsColor::from_hex_with_vc(&state_hex, &vc).unwrap().jp();
        let delta = (jp - bg_jp).abs();
        assert!(
            delta > prev_delta,
            "{name}: РєРѕРјРїРѕР·РёС†РёСЏ СЃС‚РµРєР° РЅРµ РїСЂРѕРіСЂРµСЃСЃРёРІРЅР°: |О”J'| {delta:.4} в‰¤ РїСЂРµРґ. {prev_delta:.4}"
        );
        prev_delta = delta;
    }
}

/// Kind glow: screen-СЃР»РѕРё + СЂРµС€С‘РЅРЅР°СЏ РёРЅС‚РµРЅСЃРёРІРЅРѕСЃС‚СЊ.
///
/// Р—Р°РєСЂРµРїР»СЏРµС‚ РЅРѕРІСѓСЋ СЌРјРёСЃСЃРёСЋ fx-glow-* (РІР·Р°РјРµРЅ РІС‹РІРµРґРµРЅРЅС‹С… РёР· СЃС‚Р°Р±-СЃРІРµСЂРєРё
/// Ladder@52-СЃС‚СЂРѕРє): (Р°) РЅР° С‚С‘РјРЅРѕР№ Р±Р°Р·Рµ РїР°СЃРїРѕСЂС‚Р° СЃРІРµС‡РµРЅРёРµ СЂРµС€Р°РµС‚СЃСЏ Р‘Р•Р—
/// РґРµРіСЂР°РґР°С†РёРё, halo = РїРµСЂ-С‚РµРјРЅС‹Р№ СЏРєРѕСЂСЊ РёСЃС‚РѕС‡РЅРёРєР°, О± в€€ (0, 1], С„Р°РєС‚РёС‡РµСЃРєРёР№ С€Р°Рі
/// РІ РґРѕРїСѓСЃРєРµ РєРІР°РЅС‚РѕРІР°РЅРёСЏ РѕС‚ РєРѕРЅС‚СЂР°РєС‚РЅРѕР№ СЃС‚СѓРїРµРЅРё Base; (Р±) РЅР° Р±РµР»РѕРј С„РѕРЅРµ
/// Р±РµР»РѕРµ РЅРµР№С‚СЂР°Р»СЊРЅРѕРµ СЃРІРµС‡РµРЅРёРµ РІРѕР·РІСЂР°С‰Р°РµС‚ СЏРІРЅС‹Р№ typed target status (РЅР° Р±РµР»РѕРј
/// screen вЂ” point-no-op reference-РїСЂРѕС„РёР»СЏ), РЅРµ РјРѕР»С‡Р°РЅРёРµ Рё РЅРµ РѕС€РёР±РєР°.
#[test]
fn glow_roles_resolve_screen_layers() {
    let cfg = labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("С„РёРєСЃС‚СѓСЂР° labui РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");

    // C7e hard-cut: legacy CAM16 solver removed. Non-noop glow inputs now
    // resolve as GlowIndeterminate under all profiles. Only exact noop
    // (white-on-white) returns Determinate with ExactNoopUnreachable status.

    // (Р°) С‚С‘РјРЅР°СЏ Р±Р°Р·Р°: non-noop glow is now Indeterminate.
    let bg_dark = BgInput::solid("#101012").unwrap();
    let vc_dark = ViewingConditions::dim_surround();
    let set = resolve_named_set(&bg_dark, &table, &vc_dark)
        .expect("РІР°Р»РёРґРЅС‹Р№ Glow-РєРѕРЅС‚СЂР°РєС‚ РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fx-glow-brand")
        .expect("fx-glow-brand РІ РЅР°Р±РѕСЂРµ");
    match res {
        Resolved::GlowIndeterminate(indet) => {
            assert_eq!(
                indet.evidence(),
                crate::numerics::NumericalIndeterminacyV1::SoundBoundUnavailable,
                "non-noop glow must be indeterminate after C7e hard-cut"
            );
        }
        other => panic!(
            "fx-glow-brand РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ GlowIndeterminate РїРѕСЃР»Рµ C7e hard-cut, РїРѕР»СѓС‡РµРЅРѕ {other:?}"
        ),
    }

    // (Р±) Р±РµР»РѕРµ СЃРІРµС‡РµРЅРёРµ РЅР° Р±РµР»РѕРј вЂ” exact noop, remains Determinate.
    let bg_white = BgInput::solid("#FFFFFF").unwrap();
    let set = resolve_named_set(&bg_white, &table, &ViewingConditions::srgb())
        .expect("РІР°Р»РёРґРЅС‹Р№ Glow-РєРѕРЅС‚СЂР°РєС‚ РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ");
    let (_, res) = set
        .iter()
        .find(|(n, _)| n == "fx-glow-neutral")
        .expect("fx-glow-neutral РІ РЅР°Р±РѕСЂРµ");
    match res {
        Resolved::Glow(g) => {
            assert_eq!(
                g.target_status(),
                crate::glow::GlowTargetStatus::ExactNoopUnreachable,
                "Р±РµР»РѕРµ СЃРІРµС‡РµРЅРёРµ РЅР° Р±РµР»РѕРј вЂ” exact noop"
            );
            assert_eq!(
                g.halo_composite_hex(),
                "#FFFFFF",
                "screen РЅР°Рґ Р±РµР»С‹Рј вЂ” С‚РѕР¶РґРµСЃС‚РІРѕ"
            );
        }
        other => {
            panic!(
                "fx-glow-neutral РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Resolved::Glow (exact noop), РїРѕР»СѓС‡РµРЅРѕ {other:?}"
            )
        }
    }
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// 5. РџСѓСЃС‚РѕР№ РєРѕРЅС‚СЂР°РєС‚ РѕС‚РєР»РѕРЅСЏРµС‚СЃСЏ РЅР° Р·Р°РіСЂСѓР·РєРµ.
//    РђРіРЅРѕСЃС‚РёС‡РЅРѕСЃС‚СЊ: СЏРґСЂРѕ РЅРµ Р·РЅР°РµС‚ СЂРѕР»РµР№ вЂ” РєРѕРЅС„РёРі РЅРµСЃС‘С‚ РЎР’РћР™ СЃР»РѕРІР°СЂСЊ; РіРѕР»С‹Р№
//    РєРѕРЅС‚СЂР°РєС‚ (Р±РµР· СЂРѕР»РµР№ Рё Р°Р»РёР°СЃРѕРІ) вЂ” С‡РµСЃС‚РЅР°СЏ РѕС€РёР±РєР° РЅР° Р·Р°РіСЂСѓР·РєРµ, РЅРµ С‚РёС…РёР№ РїСЂРёС‘Рј.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

#[test]
fn empty_contract_is_rejected_at_load() {
    // Р“РѕР»С‹Р№ РєРѕРЅС‚СЂР°РєС‚: РІР°Р»РёРґРЅР°СЏ СЃС‚СЂСѓРєС‚СѓСЂР°, РЅРѕ РЅРё СЂРѕР»РµР№, РЅРё Р°Р»РёР°СЃРѕРІ вЂ” С‡РµСЃС‚РЅР°СЏ
    // РѕС€РёР±РєР° РќРђ Р—РђР“Р РЈР—РљР• (validate = РєРѕРјРїРёР»СЏС†РёСЏ), РЅРµ С‚РёС…РёР№ РїСѓСЃС‚РѕР№ РїСЂРёС‘Рј.
    let mut empty = labui_reference();
    empty.roles.clear();
    empty.aliases.clear();
    assert_eq!(
        empty.validate(),
        Err(ConfigError::EmptyContract),
        "РєРѕРЅС„РёРі Р±РµР· СЂРѕР»РµР№/Р°Р»РёР°СЃРѕРІ РѕР±СЏР·Р°РЅ РѕС‚РєР»РѕРЅСЏС‚СЊСЃСЏ"
    );
    assert_eq!(
        empty.compile_named_role_table().err(),
        Some(ConfigError::EmptyContract),
        "РѕС‚РєР°Р· РЅР° РєРѕРјРїРёР»СЏС†РёРё (Р·Р°РіСЂСѓР·РєРµ), РЅРµ РЅР° РёСЃРїРѕР»СЊР·РѕРІР°РЅРёРё"
    );

    // РЎРѕРѕР±С‰РµРЅРёРµ РїРѕ-СЂСѓСЃСЃРєРё Рё РЅР°Р·С‹РІР°РµС‚ РІС‹С…РѕРґ.
    let msg = ConfigError::EmptyContract.to_string();
    assert!(
        msg.contains("РєРѕРЅС‚СЂР°РєС‚ РїСѓСЃС‚") && msg.contains("roles"),
        "СЃРѕРѕР±С‰РµРЅРёРµ РїРѕ-СЂСѓСЃСЃРєРё Рё РїРѕРґСЃРєР°Р·С‹РІР°РµС‚ РІС‹С…РѕРґ: {msg:?}"
    );
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// РњР°С‚РµСЂРёР°Р» (whitepaper, В«РўРѕС‡РµС‡РЅС‹Рµ РєРѕРјРїРѕР·РёС†РёРёВ»): РґРІСѓС…СЃР»РѕР№РЅС‹Р№ РєРѕРЅС‚СЂР°РєС‚ В«С‚РёРЅС‚ 01 (О±) + Р±Р°Р·Р° 02В» СЃ Р’Р«Р’Р•Р”Р•РќРќРћР™ О±.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// Р—Р°РјРµРЅРёС‚СЊ СЂРѕР»СЊ РЅР° РїСЂРѕРёР·РІРѕР»СЊРЅС‹Р№ СЂРµС†РµРїС‚ Рё РІРµСЂРЅСѓС‚СЊ РµС‘ СЂРµР·РѕР»РІ РЅР° `bg_hex`/`vc`.
fn resolve_role_recipe(
    role: &str,
    recipe: RoleRecipe,
    bg_hex: &str,
    vc: &ViewingConditions,
) -> Resolved {
    let cfg = with_role_recipe(role, recipe);
    let table = cfg
        .compile_named_role_table()
        .expect("material-РєРѕРЅС„РёРі РєРѕРјРїРёР»РёСЂСѓРµС‚СЃСЏ");
    let bg = BgInput::solid(bg_hex).unwrap();
    resolve_named_set(&bg, &table, vc)
        .expect("РІР°Р»РёРґРЅС‹Р№ material-РєРѕРЅС‚СЂР°РєС‚ РѕР±СЏР·Р°РЅ СЂРµР·РѕР»РІРёС‚СЊСЃСЏ")
        .into_iter()
        .find(|(n, _)| n == role)
        .map(|(_, r)| r)
        .expect("СЂРѕР»СЊ РїСЂРёСЃСѓС‚СЃС‚РІСѓРµС‚ РІ СЂРµР·РѕР»РІРµ")
}

/// РќРµР№С‚СЂР°Р»СЊРЅС‹Р№ material-СЂРµС†РµРїС‚ РЅР° РґР°РЅРЅРѕРј |О”J'| С‚РѕРЅР° (РѕР±Рµ С‚РµРјС‹).
fn neutral_material(tone: f64, floor: Floor) -> RoleRecipe {
    RoleRecipe::Material {
        source: LadderSource::Neutral(NeutralPick::Mid),
        tone_light: tone,
        tone_dark: tone,
        floor,
    }
}

/// Р РµР·РѕР»РІ РЅРµР№С‚СЂР°Р»СЊРЅРѕРіРѕ РјР°С‚РµСЂРёР°Р»Р° РЅР° Р±РµР»РѕРј С„РѕРЅРµ (СЃРІРµС‚Р»Р°СЏ С‚РµРјР°).
fn material_on_white(tone: f64) -> Resolved {
    resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(tone, Floor::AaText),
        "#FFFFFF",
        &ViewingConditions::srgb(),
    )
}

/// Р”РІСѓС…СЃР»РѕР№РЅРѕСЃС‚СЊ + СЃРѕР»РёРґ-РєР°РЅРѕРЅ Р±Р°Р№С‚-С‚РѕС‡РµРЅ + AA-РіР°СЂР°РЅС‚РёСЏ РґРµСЂР¶РёС‚СЃСЏ.
#[test]
fn material_two_layer_solid_canon_byte_exact_and_guaranteed() {
    let res = material_on_white(12.0);
    let Resolved::Material(m) = &res else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Material, РїРѕР»СѓС‡РµРЅРѕ {res:?}");
    };
    // РўРёРЅС‚ 01 = Р±Р°Р·Р° 02 = СЃРѕР»РёРґ-РєР°РЅРѕРЅ (РѕРґРёРЅ С‚РѕРЅ).
    assert_eq!(m.tint_hex(), m.base_hex(), "01 Рё 02 вЂ” РѕРґРёРЅ С‚РѕРЅ");
    assert_eq!(
        m.tint_hex(),
        m.solid_hex(),
        "СЃРѕР»РёРґ-РєР°РЅРѕРЅ = С‚РѕРЅ"
    );
    // РљРѕРјРїРѕР·РёС‚ 01-РЅР°Рґ-02 Р‘РђР™Рў-РўРћР§РќРћ СЂР°РІРµРЅ С‚РѕРЅСѓ (РєРѕРјРїРѕР·РёС‚ T РЅР°Рґ T РµСЃС‚СЊ T).
    let solid = crate::alpha::composite_hex(m.tint_hex(), m.alpha(), m.base_hex()).unwrap();
    assert_eq!(
        &solid,
        m.solid_hex(),
        "СЃРѕР»РёРґ-РєР°РЅРѕРЅ 01-РЅР°Рґ-02 СЂР°Р·РѕС€С‘Р»СЃСЏ СЃ С‚РѕРЅРѕРј"
    );
    // О± РІС‹РІРµРґРµРЅР° РІ (0,1] Рё РґРµСЂР¶РёС‚ РїРѕР».
    assert!(
        m.alpha() > 0.0 && m.alpha() <= 1.0,
        "О± РІРЅРµ (0,1]: {}",
        m.alpha()
    );
    assert_eq!(
        m.alpha_status(),
        crate::material::MaterialAlphaStatusV1::Satisfied,
        "AA-floor РѕР±СЏР·Р°РЅ РёРјРµС‚СЊ typed satisfied status"
    );
    assert!(m.worst_contrast() >= m.floor() - 1e-9, "worst < floor");
    assert!((m.floor() - 4.5).abs() < 1e-12, "AA-text РїРѕР» = 4.5");
}

/// Р“Р°СЂР°РЅС‚РёСЏ С‡РёС‚Р°РµРјРѕСЃС‚Рё РїРµСЂРµСЃС‡РёС‚С‹РІР°РµРјР° РёР· СЌРјРёС‚РёСЂРѕРІР°РЅРЅС‹С… `01`/`02`: public core
/// recheck РїРѕР±РёС‚РЅРѕ СЃРѕРІРїР°РґР°РµС‚ СЃ СЃРѕС…СЂР°РЅС‘РЅРЅС‹Рј conservative verdict, Р° РЅРµР·Р°РІРёСЃРёРјС‹Рµ
/// byte-scale consumer probes РЅРµ РѕРїСѓСЃРєР°СЋС‚СЃСЏ РЅРёР¶Рµ РЅРµРіРѕ.
#[test]
fn material_guarantee_recomputable_over_worst_backdrop() {
    let res = material_on_white(15.0);
    let Resolved::Material(m) = &res else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Material");
    };
    let tint = crate::spaces::srgb::srgb_encoded_from_hex(m.tint_hex()).unwrap();
    let recomputed = crate::material::worst_contrast_encoded(
        tint,
        m.alpha(),
        &crate::material::BackdropBox::FULL,
        m.pole(),
    )
    .unwrap();
    assert_eq!(recomputed.to_bits(), m.worst_contrast().to_bits());

    // Independent official scalar order. The old version of this test called
    // alpha::composite_over_encoded, which is the normalized-expanded profile
    // and therefore could not prove material consumer parity.
    let pole_lum = if matches!(m.pole(), crate::material::Pole::White) {
        1.0
    } else {
        0.0
    };
    let probes = [0.0, 0.039_28, 0.039_280_000_000_000_01, 0.5, 1.0];
    let mut measured_min = f64::INFINITY;
    for red in probes {
        for green in probes {
            for blue in probes {
                let background = [red, green, blue];
                let composite = core::array::from_fn(|channel| {
                    let tint_byte = (tint[channel] * 255.0).round();
                    let background_byte_scale = background[channel] * 255.0;
                    (background_byte_scale + m.alpha() * (tint_byte - background_byte_scale))
                        / 255.0
                });
                measured_min = measured_min.min(crate::spaces::srgb::relative_luminance_ratio(
                    pole_lum,
                    crate::spaces::srgb::encoded_srgb_relative_luminance(composite),
                ));
            }
        }
    }
    assert!(m.worst_contrast() <= measured_min);
    assert!(
        m.worst_contrast() >= 4.5,
        "conservative verdict РЅРёР¶Рµ AA-РїРѕР»Р°"
    );
}

/// РќРµР№С‚СЂР°Р»СЊРЅС‹Р№ РјР°С‚РµСЂРёР°Р» Р‘РђР™Рў-РІ-Р±Р°Р№С‚ РїРµСЂРµРёСЃРїРѕР»СЊР·СѓРµС‚ С‚РѕРЅ dj-anchor (С‚Р° Р¶Рµ С„РёР·РёРєР°
/// РїРѕРІРµСЂС…РЅРѕСЃС‚Рё), Р° РЅРµ РёР·РѕР±СЂРµС‚Р°РµС‚ РІС‚РѕСЂРѕР№ РїСѓС‚СЊ.
#[test]
fn neutral_material_tone_matches_dj_anchor() {
    let vc = ViewingConditions::srgb();
    let mat = resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(14.0, Floor::AaText),
        "#FFFFFF",
        &vc,
    );
    let Resolved::Material(m) = &mat else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Material");
    };
    let dj = resolve_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::DjAnchor {
            light: 14.0,
            dark: 14.0,
        },
        "#FFFFFF",
        &vc,
    );
    let dj_hex = dj
        .solved()
        .expect("dj-anchor СЂРµС€Р°РµС‚СЃСЏ РІ С†РІРµС‚")
        .hex();
    assert_eq!(
        m.tint_hex(),
        dj_hex,
        "РЅРµР№С‚СЂР°Р»СЊРЅС‹Р№ РјР°С‚РµСЂРёР°Р» РѕР±СЏР·Р°РЅ РЅРµСЃС‚Рё С‚РѕС‚ Р¶Рµ С‚РѕРЅ, С‡С‚Рѕ dj-anchor"
    );
}

/// РЎРµРјРµР№РЅС‹Р№ (brand) РјР°С‚РµСЂРёР°Р» РЅРµСЃС‘С‚ РћРўРўР•РќРћРљ СЃРµРјСЊРё вЂ” РµРіРѕ С‚РѕРЅ РѕС‚Р»РёС‡Р°РµС‚СЃСЏ РѕС‚
/// РЅРµР№С‚СЂР°Р»СЊРЅРѕРіРѕ РЅР° С‚РѕРј Р¶Рµ |О”J'| (Р°РєС†РµРЅС‚-СЃС‚РµРєР»Рѕ СЂР°Р·Р±Р»РѕРєРёСЂРѕРІР°РЅРѕ).
#[test]
fn accent_material_tone_carries_family_hue() {
    let vc = ViewingConditions::srgb();
    let neutral = material_on_white(22.0);
    let brand = resolve_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Brand,
            tone_light: 22.0,
            tone_dark: 22.0,
            floor: Floor::AaText,
        },
        "#FFFFFF",
        &vc,
    );
    let (Resolved::Material(n), Resolved::Material(b)) = (&neutral, &brand) else {
        panic!("РѕР¶РёРґР°Р»РёСЃСЊ Material");
    };
    assert_ne!(
        n.tint_hex(),
        b.tint_hex(),
        "brand-РјР°С‚РµСЂРёР°Р» РѕР±СЏР·Р°РЅ РѕС‚Р»РёС‡Р°С‚СЊСЃСЏ РѕС‚ РЅРµР№С‚СЂР°Р»СЊРЅРѕРіРѕ (РѕС‚С‚РµРЅРѕРє СЃРµРјСЊРё)"
    );
}

/// РџРѕСЂСЏРґРѕРє С‚РёСЂРѕРІ Р’Р«Р’РћР”РРўРЎРЇ С„РёР·РёРєРѕР№, РЅРµ РїРѕРґР±РѕСЂРѕРј: РЅР° СЃРІРµС‚Р»РѕР№ С‚РµРјРµ С‚РѕРЅ РґР°Р»СЊС€Рµ РѕС‚
/// Р±РµР»РѕРіРѕ (РєСЂСѓРїРЅРµРµ |О”J'| = base) С‚СЂРµР±СѓРµС‚ РџР›РћРўРќР•Р• О±, С‡РµРј Р±Р»РёР¶Рµ (subtle).
#[test]
fn material_base_denser_than_subtle_light_theme() {
    let alpha_of = |tone: f64| match material_on_white(tone) {
        Resolved::Material(m) => m.alpha(),
        other => panic!("РѕР¶РёРґР°Р»СЃСЏ Material, РїРѕР»СѓС‡РµРЅРѕ {other:?}"),
    };
    let subtle = alpha_of(6.0);
    let base = alpha_of(26.0);
    assert!(
        base > subtle,
        "base ({base}) РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РїР»РѕС‚РЅРµРµ subtle ({subtle})"
    );
}

/// RED-proof С‚РѕРЅР°: СЂР°Р·РЅС‹Р№ |О”J'| РѕР±СЏР·Р°РЅ РґР°С‚СЊ СЂР°Р·РЅС‹Р№ С‚РѕРЅ (СЂРµС†РµРїС‚ РЅРµ СЃР»РµРї Рє С‚РёСЂСѓ).
#[test]
fn material_bites_on_tone_mutation() {
    let tone_of = |tone: f64| match material_on_white(tone) {
        Resolved::Material(m) => m.tint_hex().to_string(),
        other => panic!("РѕР¶РёРґР°Р»СЃСЏ Material, РїРѕР»СѓС‡РµРЅРѕ {other:?}"),
    };
    assert_ne!(
        tone_of(8.0),
        tone_of(28.0),
        "RED-proof: СЂР°Р·РЅС‹Р№ |О”J'| РґР°Р» РѕРґРёРЅР°РєРѕРІС‹Р№ С‚РѕРЅ вЂ” СЂРµС†РµРїС‚ СЃР»РµРї Рє С‚РёСЂСѓ"
    );
}

/// РўРѕРЅ-Р±Р°Р·Р° СЂР°Р·Р»РёС‡РёРјР° РѕС‚ С„РѕРЅР° (|О”J'| в‰€ С†РµР»СЊ) Рё РѕС‚РјРµС‡РµРЅР° distinct.
#[test]
fn material_tone_is_distinguishable_from_bg() {
    let res = material_on_white(15.0);
    let Resolved::Material(m) = &res else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Material");
    };
    assert!(
        m.distinct(),
        "С‚РѕРЅ РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РѕС‚Р»РёС‡РёРј РѕС‚ С„РѕРЅР° РЅР° 8-Р±РёС‚РЅРѕР№ СЃРµС‚РєРµ"
    );
    assert!(
        (m.achieved_dj() - 15.0).abs() < 2.5,
        "achieved_dj {} РґР°Р»С‘Рє РѕС‚ С†РµР»Рё 15.0",
        m.achieved_dj()
    );
}

/// РўС‘РјРЅР°СЏ С‚РµРјР°: С‚С‘РјРЅР°СЏ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ в†’ Р±РµР»С‹Р№ РєРѕРјРјРёС‚-РїРѕР»СЋСЃ, РіР°СЂР°РЅС‚РёСЏ РґРµСЂР¶РёС‚СЃСЏ.
#[test]
fn material_dark_theme_white_pole_guaranteed() {
    let res = resolve_role_recipe(
        "fill-brand-secondary",
        neutral_material(15.0, Floor::AaText),
        "#101012",
        &ViewingConditions::dim_surround(),
    );
    let Resolved::Material(m) = &res else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ Material");
    };
    assert!(
        matches!(m.pole(), crate::material::Pole::White),
        "С‚С‘РјРЅР°СЏ РїРѕРІРµСЂС…РЅРѕСЃС‚СЊ РѕР±СЏР·Р°РЅР° РєРѕРјРјРёС‚РёС‚СЊ Р±РµР»С‹Р№ РїРѕР»СЋСЃ"
    );
    assert_eq!(
        m.alpha_status(),
        crate::material::MaterialAlphaStatusV1::Satisfied,
        "AA-floor РѕР±СЏР·Р°РЅ РёРјРµС‚СЊ typed satisfied status Рё РЅР° С‚С‘РјРЅРѕР№ С‚РµРјРµ"
    );
}

/// Р’Р°Р»РёРґР°С‚РѕСЂ: material Р±РµР· РїРѕР»Р° С‡РёС‚Р°РµРјРѕСЃС‚Рё РѕС‚РІРµСЂРіР°РµС‚СЃСЏ РЅР° Р·Р°РіСЂСѓР·РєРµ.
#[test]
fn material_floor_none_rejected() {
    let cfg = with_role_recipe("fill-brand-secondary", neutral_material(10.0, Floor::None));
    assert!(
        matches!(
            cfg.validate(),
            Err(ConfigError::MaterialFloorRequired { role }) if role == "fill-brand-secondary"
        ),
        "floor=none РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚"
    );
}

/// Р’Р°Р»РёРґР°С‚РѕСЂ: РЅРµРїРѕР»РѕР¶РёС‚РµР»СЊРЅС‹Р№ |О”J'| С‚РѕРЅР° РѕС‚РІРµСЂРіР°РµС‚СЃСЏ (РЅРµС‚ СЂР°Р·Р»РёС‡РёРјРѕР№ РїРѕРІРµСЂС…РЅРѕСЃС‚Рё).
#[test]
fn material_non_positive_tone_rejected() {
    let cfg = with_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Neutral(NeutralPick::Mid),
            tone_light: 0.0,
            tone_dark: 10.0,
            floor: Floor::AaText,
        },
    );
    assert!(
        matches!(
            cfg.validate(),
            Err(ConfigError::OutOfBounds { handle, .. }) if handle == "roles.fill-brand-secondary.tone_light"
        ),
        "tone_light=0 РѕР±СЏР·Р°РЅ Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚"
    );
}

/// Р’Р°Р»РёРґР°С‚РѕСЂ: material СЃРѕ СЃСЃС‹Р»РєРѕР№ РЅР° РЅРµСЃСѓС‰РµСЃС‚РІСѓСЋС‰РµРµ СЃРµРјРµР№СЃС‚РІРѕ РѕС‚РІРµСЂРіР°РµС‚СЃСЏ.
#[test]
fn material_unknown_family_rejected() {
    let cfg = with_role_recipe(
        "fill-brand-secondary",
        RoleRecipe::Material {
            source: LadderSource::Family("РЅРµС‚-С‚Р°РєРѕРіРѕ".to_string()),
            tone_light: 10.0,
            tone_dark: 10.0,
            floor: Floor::AaText,
        },
    );
    assert!(
        matches!(cfg.validate(), Err(ConfigError::UnknownFamily { .. })),
        "СЃСЃС‹Р»РєР° РЅР° РЅРµСЃСѓС‰РµСЃС‚РІСѓСЋС‰РµРµ СЃРµРјРµР№СЃС‚РІРѕ РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РѕС‚РІРµСЂРіРЅСѓС‚Р°"
    );
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// C7d characterization corpus: РєР°Р¶РґС‹Р№ РїСѓР±Р»РёС‡РЅС‹Р№ construction path РјР°С‚РµСЂРёР°Р»Р°
// Р·Р°РєСЂРµРїР»С‘РЅ Р‘РРў-РІ-Р±РёС‚ РЅР° С‚РµРєСѓС‰РµР№ С„РёР·РёРєРµ Р”Рћ lowering. РћРЅ РѕР±СЏР·Р°РЅ РїСЂРѕР№С‚Рё Р±РµР·
// РёР·РјРµРЅРµРЅРёР№ РїРѕСЃР»Рµ РїРµСЂРµРЅРѕСЃР° РёСЃРїРѕР»РЅРµРЅРёСЏ РІ РѕР±С‰РёР№ Program-РїСѓС‚СЊ: РїСЂРѕРїСѓС‰РµРЅРЅС‹Р№
// construction path, СЃРјРµРЅР° РєРѕРјРїРѕСЃРёС‚РѕСЂР°/РїРѕСЂСЏРґРєР°, РѕРґРёРЅ backdrop РІРјРµСЃС‚Рѕ РєРѕСЂРёРґРѕСЂР°
// РёР»Рё РґРµРіСЂР°РґР°С†РёСЏ Р±РµР· СЃРІРёРґРµС‚РµР»СЊСЃС‚РІР° Р»РѕРјР°СЋС‚ СЌС‚Рё РїРёРЅС‹.
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// РћРґРёРЅ Р·Р°РєСЂРµРїР»С‘РЅРЅС‹Р№ РІРµРєС‚РѕСЂ РєРѕСЂРїСѓСЃР°: РєРѕРЅС‚РµРєСЃС‚, РІС…РѕРґ Рё С‚РѕС‡РЅС‹Рµ Р±РёС‚РѕРІС‹Рµ РІС‹С…РѕРґС‹.
struct MaterialCorpusVector {
    label: &'static str,
    source: fn() -> LadderSource,
    tone_light: f64,
    tone_dark: f64,
    floor: Floor,
    vc: fn() -> ViewingConditions,
    bg_hex: &'static str,
    tone_hex: &'static str,
    alpha_bits: u64,
    worst_bits: u64,
    achieved_dj_bits: u64,
    pole_white: bool,
}

/// РџРѕР»РЅС‹Р№ РєРѕСЂРїСѓСЃ: РІСЃРµ РёСЃС‚РѕС‡РЅРёРєРё (Brand / Family / РІСЃРµ РїСЏС‚СЊ NeutralPick) РІРѕ РІСЃРµС…
/// С‡РµС‚С‹СЂС‘С… С‚РµРјР°С… РїР°СЃРїРѕСЂС‚Р° РїР»СЋСЃ СЃРµСЂС‹Р№ С„РѕРЅ. РЎРјРµС€Р°РЅРЅС‹Рµ tone_lightв‰ tone_dark Рё РѕР±Р°
/// РїРѕР»Р° (AaText/AaUi) РІС…РѕРґСЏС‚ РІ РІС‹Р±РѕСЂРєСѓ.
fn material_characterization_corpus() -> Vec<MaterialCorpusVector> {
    fn srgb() -> ViewingConditions {
        ViewingConditions::srgb()
    }
    fn dim() -> ViewingConditions {
        ViewingConditions::dim_surround()
    }
    fn srgb_ic() -> ViewingConditions {
        ViewingConditions::srgb_high_contrast()
    }
    fn dim_ic() -> ViewingConditions {
        ViewingConditions::dim_surround_high_contrast()
    }
    fn mid() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Mid)
    }
    fn light() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Light)
    }
    fn dark() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Dark)
    }
    fn edge() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Edge)
    }
    fn inverted() -> LadderSource {
        LadderSource::Neutral(NeutralPick::Inverted)
    }
    fn brand() -> LadderSource {
        LadderSource::Brand
    }
    fn purple() -> LadderSource {
        LadderSource::Family("purple".to_string())
    }
    #[rustfmt::skip]
    let corpus = vec![
        // в”Ђв”Ђ srgb (СЃРІРµС‚Р»Р°СЏ), #FFFFFF в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
        MaterialCorpusVector { label: "neutral-mid/srgb", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D6D6E0", alpha_bits: 0x3fe14d5bdf014d24, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4028316ec16bfd18, pole_white: false },
        MaterialCorpusVector { label: "neutral-light/srgb", source: light, tone_light: 9.0, tone_dark: 9.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#E1E1E1", alpha_bits: 0x3fd953f339f0f17d, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x4021d4eb3ff155d0, pole_white: false },
        MaterialCorpusVector { label: "neutral-dark/srgb", source: dark, tone_light: 11.0, tone_dark: 11.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D9D9E3", alpha_bits: 0x3fda2c1ec0af1ef2, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x40264854a4095b68, pole_white: false },
        MaterialCorpusVector { label: "neutral-edge/srgb", source: edge, tone_light: 10.0, tone_dark: 10.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#DDDDE6", alpha_bits: 0x3fe0c302b2c13d45, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4023d1b0d337e540, pole_white: false },
        MaterialCorpusVector { label: "neutral-inverted/srgb", source: inverted, tone_light: 13.0, tone_dark: 13.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#D3D3DD", alpha_bits: 0x3fe18c1bebb958a9, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x402a1ed191c26ab8, pole_white: false },
        MaterialCorpusVector { label: "brand/srgb", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#B5BAC1", alpha_bits: 0x3fe4086250007280, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4035f37bf4fa5200, pole_white: false },
        MaterialCorpusVector { label: "family-purple/srgb", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: srgb, bg_hex: "#FFFFFF", tone_hex: "#CEC8D2", alpha_bits: 0x3fdc3523dcf40701, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x403027ede6ed1abc, pole_white: false },
        // в”Ђв”Ђ dim (С‚С‘РјРЅР°СЏ), #101012 в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
        MaterialCorpusVector { label: "neutral-mid/dim", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#2D2D33", alpha_bits: 0x3fe4d1e7fe28d58c, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402c088852a3aeb1, pole_white: true },
        MaterialCorpusVector { label: "neutral-light/dim", source: light, tone_light: 9.0, tone_dark: 9.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#232323", alpha_bits: 0x3fdedf445a6c8d35, worst_bits: 0x4008000000000000, achieved_dj_bits: 0x4022148cd5a1d431, pole_white: true },
        MaterialCorpusVector { label: "neutral-dark/dim", source: dark, tone_light: 11.0, tone_dark: 11.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#27272D", alpha_bits: 0x3fdf81fdd297a6c1, worst_bits: 0x4008000000000000, achieved_dj_bits: 0x402668319c957807, pole_white: true },
        MaterialCorpusVector { label: "neutral-edge/dim", source: edge, tone_light: 10.0, tone_dark: 10.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#21262B", alpha_bits: 0x3fe40b0c57df5fa6, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x40243de1f7538c49, pole_white: true },
        MaterialCorpusVector { label: "neutral-inverted/dim", source: inverted, tone_light: 13.0, tone_dark: 13.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#2B2B31", alpha_bits: 0x3fe49f84665de8cc, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402a2a534bb93399, pole_white: true },
        MaterialCorpusVector { label: "brand/dim", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: dim, bg_hex: "#101012", tone_hex: "#33363C", alpha_bits: 0x3fe5afa483913ece, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x4031d5399ccd0c3e, pole_white: true },
        MaterialCorpusVector { label: "family-purple/dim", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: dim, bg_hex: "#101012", tone_hex: "#343037", alpha_bits: 0x3fe083b9c39a26ac, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x402fde43cb629035, pole_white: true },
        // в”Ђв”Ђ srgb-ic в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
        MaterialCorpusVector { label: "neutral-mid/srgb-ic", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#D6D6E0", alpha_bits: 0x3fe14d5bdf014d24, worst_bits: 0x4012000000000002, achieved_dj_bits: 0x4028316ec16bfd18, pole_white: false },
        MaterialCorpusVector { label: "brand/srgb-ic", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#B5BAC2", alpha_bits: 0x3fe40647c3868b55, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x4035eb3157a096e8, pole_white: false },
        MaterialCorpusVector { label: "family-purple/srgb-ic", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: srgb_ic, bg_hex: "#FFFFFF", tone_hex: "#CEC8D2", alpha_bits: 0x3fdc3523dcf40701, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x403027ede6ed1abc, pole_white: false },
        // в”Ђв”Ђ dim-ic в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
        MaterialCorpusVector { label: "neutral-mid/dim-ic", source: mid, tone_light: 12.0, tone_dark: 14.0, floor: Floor::AaText, vc: dim_ic, bg_hex: "#101012", tone_hex: "#2D2D33", alpha_bits: 0x3fe4d1e7fe28d58c, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x402c088852a3aeb1, pole_white: true },
        MaterialCorpusVector { label: "brand/dim-ic", source: brand, tone_light: 22.0, tone_dark: 18.0, floor: Floor::AaText, vc: dim_ic, bg_hex: "#101012", tone_hex: "#33373C", alpha_bits: 0x3fe5c37fb50b1c72, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x403222d5f10b5a2e, pole_white: true },
        MaterialCorpusVector { label: "family-purple/dim-ic", source: purple, tone_light: 16.0, tone_dark: 16.0, floor: Floor::AaUi, vc: dim_ic, bg_hex: "#101012", tone_hex: "#343037", alpha_bits: 0x3fe083b9c39a26ac, worst_bits: 0x4008000000000001, achieved_dj_bits: 0x402fde43cb629035, pole_white: true },
        // в”Ђв”Ђ СЃРµСЂС‹Р№ С„РѕРЅ: РїРѕР»СЋСЃ РїРµСЂРµРІРѕСЂР°С‡РёРІР°РµС‚СЃСЏ РЅР° СЃРІРµС‚Р»РѕР№ С‚РµРјРµ в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
        MaterialCorpusVector { label: "neutral-mid/gray-bg", source: mid, tone_light: 12.0, tone_dark: 12.0, floor: Floor::AaText, vc: srgb, bg_hex: "#7F7F7F", tone_hex: "#61616A", alpha_bits: 0x3febbb7a9c1a37aa, worst_bits: 0x4012000000000000, achieved_dj_bits: 0x40283945e563ccc8, pole_white: true },
    ];
    corpus
}

/// Р‘РёС‚-РІ-Р±РёС‚ РєРѕСЂРїСѓСЃ РІСЃРµС… construction paths РєРѕРЅС„РёРіР°: tone hex, О±, worst,
/// |О”J'|, РїРѕР»СЋСЃ, СЃС‚Р°С‚СѓСЃ Рё СЃРѕРіР»Р°СЃРѕРІР°РЅРЅС‹Р№ bracket. Р›СЋР±РѕРµ РёР·РјРµРЅРµРЅРёРµ
/// РєРѕРјРїРѕСЃРёС‚РѕСЂР°, РїРѕСЂСЏРґРєР° РѕРїРµСЂР°С†РёР№, РєРѕСЂРёРґРѕСЂР° РёР»Рё РІС‹Р±РѕСЂР° РєР°РЅРґРёРґР°С‚Р° Р»РѕРјР°РµС‚ РїРёРЅС‹.
#[test]
fn material_lowering_characterization_corpus_is_bit_stable() {
    for vector in material_characterization_corpus() {
        let recipe = RoleRecipe::Material {
            source: (vector.source)(),
            tone_light: vector.tone_light,
            tone_dark: vector.tone_dark,
            floor: vector.floor,
        };
        let resolved = resolve_role_recipe(
            "fill-brand-secondary",
            recipe,
            vector.bg_hex,
            &(vector.vc)(),
        );
        let Resolved::Material(m) = &resolved else {
            panic!(
                "{}: РѕР¶РёРґР°Р»СЃСЏ Material, РїРѕР»СѓС‡РµРЅРѕ {resolved:?}",
                vector.label
            );
        };
        assert_eq!(
            m.tint_hex(),
            vector.tone_hex,
            "{}: tone drift",
            vector.label
        );
        assert_eq!(
            m.alpha().to_bits(),
            vector.alpha_bits,
            "{}: alpha drift (got 0x{:016x})",
            vector.label,
            m.alpha().to_bits()
        );
        assert_eq!(
            m.worst_contrast().to_bits(),
            vector.worst_bits,
            "{}: worst-contrast drift (got 0x{:016x})",
            vector.label,
            m.worst_contrast().to_bits()
        );
        assert_eq!(
            m.achieved_dj().to_bits(),
            vector.achieved_dj_bits,
            "{}: achieved-dj drift (got 0x{:016x})",
            vector.label,
            m.achieved_dj().to_bits()
        );
        assert_eq!(
            matches!(m.pole(), crate::material::Pole::White),
            vector.pole_white,
            "{}: pole drift",
            vector.label
        );
        assert_eq!(
            m.alpha_status(),
            crate::material::MaterialAlphaStatusV1::Satisfied,
            "{}: status drift",
            vector.label
        );
        assert!(
            !m.tone_compressed(),
            "{}: unexpected compression",
            vector.label
        );
        assert!(m.distinct(), "{}: tone must stay distinct", vector.label);
        // РЎРѕРіР»Р°СЃРѕРІР°РЅРЅРѕСЃС‚СЊ bracket-СЃРІРёРґРµС‚РµР»СЊСЃС‚РІР°: РІС‹Р±СЂР°РЅРЅР°СЏ О± вЂ” РІРµСЂС…РЅРёР№
        // РєР°РЅРґРёРґР°С‚, РЅРёР¶РЅРёР№ Р»РµР¶РёС‚ СЂРѕРІРЅРѕ РЅР° РїСЂРµРґС‹РґСѓС‰РµРј Р±РёС‚Рµ РїРѕРёСЃРєР°.
        match m.alpha_guarantee() {
            crate::material::MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                iterations,
                lower_alpha,
                upper_alpha,
                ..
            } => {
                assert_eq!(iterations, 60, "{}: bisection depth drift", vector.label);
                assert_eq!(
                    upper_alpha.to_bits(),
                    vector.alpha_bits,
                    "{}: bracket upper != selected alpha",
                    vector.label
                );
                assert!(
                    lower_alpha < upper_alpha,
                    "{}: bracket must stay ordered",
                    vector.label
                );
            }
            other => panic!(
                "{}: РѕР¶РёРґР°Р»СЃСЏ bracket, РїРѕР»СѓС‡РµРЅРѕ {other:?}",
                vector.label
            ),
        }
        // РџРѕР» РєРѕСЂРїСѓСЃР° РІС‹РїРѕР»РЅРµРЅ: worst РґРµСЂР¶РёС‚ Р·Р°РїСЂРѕС€РµРЅРЅС‹Р№ floor.
        let floor_ratio = vector.floor.min_ratio().expect("corpus floors are legal");
        assert!(
            m.worst_contrast() >= floor_ratio,
            "{}: worst {} РЅРёР¶Рµ РїРѕР»Р° {}",
            vector.label,
            m.worst_contrast(),
            floor_ratio
        );
    }
}

/// РџСЂСЏРјР°СЏ РіСЂР°РЅРёС†Р° `NamedRoleTable::new` (РІ РѕР±С…РѕРґ РєРѕРЅС„РёРіР°) СЃ `hue: None`
/// Р·Р°РєСЂРµРїР»РµРЅР° РѕС‚РґРµР»СЊРЅРѕ: РѕРЅР° РѕР±СЏР·Р°РЅР° РїРµСЂРµР¶РёС‚СЊ lowering С‚РµРј Р¶Рµ Р±РёС‚РѕРІС‹Рј РІС‹С…РѕРґРѕРј.
#[test]
fn material_direct_boundary_hue_none_is_bit_stable() {
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};
    let table = NamedRoleTable::new(
        vec![(
            "m".to_string(),
            RoleSpec::Material {
                hue: None,
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaText,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .unwrap();
    let set = resolve_named_set(
        &BgInput::solid("#FFFFFF").unwrap(),
        &table,
        &ViewingConditions::srgb(),
    )
    .unwrap();
    let Resolved::Material(m) = &set[0].1 else {
        panic!("РїСЂСЏРјР°СЏ РіСЂР°РЅРёС†Р° РѕР±СЏР·Р°РЅР° СЂРµР·РѕР»РІРёС‚СЊ Material");
    };
    assert_eq!(m.tint_hex(), "#D7D7D7");
    assert_eq!(m.alpha().to_bits(), 0x3fe14808ee3564a2);
    assert_eq!(m.worst_contrast().to_bits(), 0x4012000000000000);
    assert_eq!(m.achieved_dj().to_bits(), 0x40282274443b7010);
}

/// РўРёРїРёР·РёСЂРѕРІР°РЅРЅС‹Рµ РєРѕРЅС„Р»РёРєС‚С‹ РєРѕСЂРїСѓСЃР°: С…СЂРѕРјР°С‚РёС‡РµСЃРєРёР№ РёСЃС‚РѕС‡РЅРёРє РїСЂРё neutral-policy
/// РѕС‚РІРµСЂРіР°РµС‚СЃСЏ РЅР° РѕР±РµРёС… РіСЂР°РЅРёС†Р°С… (РєРѕРЅС„РёРі Рё РїСЂСЏРјР°СЏ С‚Р°Р±Р»РёС†Р°) СЃ РєР°РЅРѕРЅРёС‡РµСЃРєРѕР№
/// РїСЂРёС‡РёРЅРѕР№, Р° РЅРµ РёСЃРїРѕР»РЅСЏРµС‚СЃСЏ РґРµРіСЂР°РґРёСЂРѕРІР°РЅРЅРѕ.
#[test]
fn material_chromatic_source_conflicts_are_typed_on_both_boundaries() {
    use crate::semantic::{DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec};
    // РџСЂСЏРјР°СЏ РіСЂР°РЅРёС†Р°: chromatic hue + Neutral policy в†’ InvalidInput СЃ
    // РєР°РЅРѕРЅРёС‡РµСЃРєРѕР№ РїСЂРёС‡РёРЅРѕР№.
    let blue = crate::spaces::srgb::srgb_encoded_from_hex("#3E87FF").unwrap();
    let error = NamedRoleTable::new(
        vec![(
            "m".to_string(),
            RoleSpec::Material {
                hue: Some(crate::ladder::LadderTint::new([blue; 4]).unwrap()),
                tone: DjMagnitude::new(12.0, 12.0),
                floor: Floor::AaText,
            },
        )],
        Vec::new(),
        RoleChroma::Neutral,
    )
    .expect_err("chromatic material РїРѕРґ neutral-policy РѕР±СЏР·Р°РЅ РѕС‚РІРµСЂРіР°С‚СЊСЃСЏ");
    let crate::SolveFailure::InvalidInput(reason) = &error else {
        panic!("РѕР¶РёРґР°Р»СЃСЏ typed InvalidInput, РїРѕР»СѓС‡РµРЅРѕ {error:?}");
    };
    assert!(
        reason.contains(RoleSpec::INCOMPATIBLE_CHROMA_REASON),
        "РєРѕРЅС„Р»РёРєС‚ РѕР±СЏР·Р°РЅ РЅРµСЃС‚Рё РєР°РЅРѕРЅРёС‡РµСЃРєСѓСЋ РїСЂРёС‡РёРЅСѓ: {reason:?}"
    );
}
