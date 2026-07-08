//! Чтение замороженного паспорта labui — источник репрезентативных фирменных семей.
//!
//! SSOT стимулов — `crates/labcolors-wasm/tests/data/labui.config.json` (тот же
//! паспорт, что даёт `exposure_support::LABUI_ANCHORS`; но `LABUI_ANCHORS` —
//! `pub(crate)` ядра, поэтому харнесс читает JSON напрямую). Берём массив
//! `palette`: каждый элемент — семья `{ key, anchors: { light, dark, ... } }`.
//! Оттенок семьи задаёт её `light`-якорь; генератор свотчей заново нацеливает
//! люминанс, используя лишь направление оттенка.

use crate::color;
use crate::json;

/// Фирменная семья: ключ и её репрезентативный якорь оттенка.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    /// Имя семьи в паспорте (`red`, `blue`, …).
    pub key: String,
    /// Hex якоря-оттенка (`light`).
    pub anchor_hex: String,
    /// 8-битные каналы якоря.
    pub anchor_rgb: [u8; 3],
}

/// Извлечь семьи из текста паспорта, в порядке паспорта.
///
/// # Errors
/// `Err`, если JSON не разобрался, нет массива `palette`, или у семьи нет
/// строкового `key` / валидного `anchors.light`.
pub fn families_from_passport(json_text: &str) -> Result<Vec<Family>, String> {
    let root = json::parse(json_text)?;
    let palette = root
        .get("palette")
        .and_then(json::Value::as_array)
        .ok_or("в паспорте нет массива 'palette'")?;

    let mut families = Vec::with_capacity(palette.len());
    for (i, entry) in palette.iter().enumerate() {
        let key = entry
            .get("key")
            .and_then(json::Value::as_str)
            .ok_or_else(|| format!("palette[{i}]: нет строкового 'key'"))?
            .to_string();
        let anchor_hex = entry
            .get("anchors")
            .and_then(|a| a.get("light"))
            .and_then(json::Value::as_str)
            .ok_or_else(|| format!("palette[{i}] ({key}): нет 'anchors.light'"))?
            .to_string();
        let anchor_rgb = color::hex_to_rgb(&anchor_hex)?;
        families.push(Family {
            key,
            anchor_hex,
            anchor_rgb,
        });
    }

    if families.is_empty() {
        return Err("паспорт: массив 'palette' пуст".to_string());
    }
    Ok(families)
}

/// Путь к паспорту относительно корня воркспейса (для бинарника по умолчанию).
#[must_use]
pub fn default_passport_relpath() -> &'static str {
    "crates/labcolors-wasm/tests/data/labui.config.json"
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r##"{
        "brand": { "light": "#007AFF" },
        "palette": [
            { "key": "red",  "anchors": { "light": "#FF3B30", "dark": "#FF3A3A" } },
            { "key": "blue", "anchors": { "light": "#007AFF", "dark": "#4A8FFF" } }
        ]
    }"##;

    #[test]
    fn extracts_families_in_order() {
        let fams = families_from_passport(MINI).unwrap();
        assert_eq!(fams.len(), 2);
        assert_eq!(fams[0].key, "red");
        assert_eq!(fams[0].anchor_hex, "#FF3B30");
        assert_eq!(fams[0].anchor_rgb, [255, 59, 48]);
        assert_eq!(fams[1].key, "blue");
    }

    #[test]
    fn missing_palette_errs() {
        assert!(families_from_passport(r#"{"brand":{}}"#).is_err());
    }

    #[test]
    fn missing_anchor_errs() {
        let bad = r#"{ "palette": [ { "key": "x", "anchors": {} } ] }"#;
        assert!(families_from_passport(bad).is_err());
    }

    #[test]
    fn reads_real_passport_if_present() {
        // Грундинг: реальный паспорт даёт ровно 10 хроматических семей в
        // известном порядке. Пропускаем, если файл недоступен (напр. крейт
        // вынесен), — юнит-тесты выше покрывают логику независимо от ФС.
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let path = format!("{root}{}", default_passport_relpath());
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("паспорт не найден по {path}; тест ФС пропущен");
            return;
        };
        let fams = families_from_passport(&text).expect("реальный паспорт разобран");
        let keys: Vec<&str> = fams.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "red", "orange", "yellow", "green", "teal", "mint", "blue", "indigo", "purple",
                "pink"
            ]
        );
    }
}
