//! Гейт исчерпанности словаря исходов чистоты (раздел 6 контракта).
//!
//! КЛАСС БАГА, который здесь ловится: **вариант объявлен в типе, но не попал в
//! `QualityOutcomeV1::ALL`**.
//!
//! Внутрикрейтовые проверки его не видят, и это надо понимать точно:
//!
//! * `const`-ассерт в `cleanliness/outcome.rs` итерирует **по `ALL`**. Вариант,
//!   отсутствующий в `ALL`, он не посещает вовсе и потому не заметит;
//! * исчерпывающие `match` в `priority`/`key` заставят автора дописать ветви
//!   новому варианту — но ничто не заставит его добавить вариант в `ALL`;
//! * unit-тест наблюдает порядок объявления через `as u8` и потому видит
//!   только те варианты, что уже перечислены в `ALL`: пропущенный он не
//!   посетит по той же причине, что и `const`-ассерт.
//!
//! Единственный способ поймать это — прочитать **сам исходник** и сравнить
//! объявленные имена с тем, что перечислено. Тест читает
//! `src/cleanliness/outcome.rs` как текст, поэтому расхождение обнаруживается
//! независимо от того, что об этом думает код.
//!
//! Нулевых зависимостей: чистый `std`, как и прочие сканеры крейта.

use std::collections::BTreeSet;

const OUTCOME_SOURCE: &str = include_str!("../src/cleanliness/outcome.rs");

/// Имена вариантов из блока `pub enum <name> { … }`.
///
/// Разбор намеренно грубый и построчный: он обязан видеть текст так же, как
/// его видит читатель, а не так, как его понимает компилятор. Комментарии и
/// атрибуты отбрасываются, вложенных фигурных скобок в этих объявлениях нет.
fn declared_variants(source: &str, enum_name: &str) -> Vec<String> {
    let header = format!("pub enum {enum_name} {{");
    let start = source
        .find(&header)
        .unwrap_or_else(|| panic!("объявление `{enum_name}` не найдено в исходнике"))
        + header.len();
    let body_len = source[start..]
        .find("\n}")
        .unwrap_or_else(|| panic!("не найден конец объявления `{enum_name}`"));

    let mut names = Vec::new();
    for line in source[start..start + body_len].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with('#') {
            continue;
        }
        // Вариант: `Name,` либо `Name(Payload),` либо `Name = 1,`.
        let name: String = line
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && name.starts_with(char::is_uppercase) {
            names.push(name);
        }
    }
    names
}

/// Имена, перечисленные в `impl <enum>::ALL`.
fn listed_in_all(source: &str, enum_name: &str) -> Vec<String> {
    let anchor = format!("impl {enum_name} {{");
    let impl_start = source
        .find(&anchor)
        .unwrap_or_else(|| panic!("блок `impl {enum_name}` не найден"));
    let all_start = source[impl_start..]
        .find("pub const ALL:")
        .unwrap_or_else(|| panic!("`{enum_name}::ALL` не найден"))
        + impl_start;
    // Искать первую `[` нельзя: она принадлежит типу `[Self; 15]`, а не
    // литералу массива. Литерал начинается после `= [`.
    const ASSIGN: &str = "= [";
    let open = source[all_start..]
        .find(ASSIGN)
        .unwrap_or_else(|| panic!("не найдено начало массива `{enum_name}::ALL`"))
        + all_start
        + ASSIGN.len();
    let close = source[open..]
        .find("];")
        .unwrap_or_else(|| panic!("не найден конец массива `{enum_name}::ALL`"))
        + open;

    source[open..close]
        .split(',')
        .filter_map(|item| item.trim().strip_prefix("Self::"))
        .map(|item| {
            item.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Гейт: множество объявленных вариантов совпадает с перечисленным в `ALL`.
#[test]
fn every_declared_outcome_is_listed_in_all() {
    for enum_name in ["QualityOutcomeV1", "OutcomePriorityV1"] {
        let declared: BTreeSet<String> = declared_variants(OUTCOME_SOURCE, enum_name)
            .into_iter()
            .collect();
        let listed: BTreeSet<String> = listed_in_all(OUTCOME_SOURCE, enum_name)
            .into_iter()
            .collect();

        let missing: Vec<_> = declared.difference(&listed).collect();
        let extra: Vec<_> = listed.difference(&declared).collect();

        assert!(
            missing.is_empty(),
            "{enum_name}: варианты объявлены, но не перечислены в ALL: {missing:?}. \
             Именно этот случай const-ассерт пропускает — он итерирует по ALL."
        );
        assert!(
            extra.is_empty(),
            "{enum_name}: в ALL перечислено то, чего нет в объявлении: {extra:?}"
        );
        assert_eq!(
            declared.len(),
            15,
            "{enum_name}: контракт объявляет ровно 15 значений"
        );
    }
}

/// Гейт: `ALL` не содержит повторов. Дубликат имени в `ALL` не поймал бы ни
/// сравнение множеств выше, ни `const`-ассерт (у него сошлись бы длины только
/// при одновременном пропуске другого варианта, но ранги разошлись бы — а
/// здесь проверка прямая и не зависит от рангов).
#[test]
fn all_lists_each_outcome_exactly_once() {
    for enum_name in ["QualityOutcomeV1", "OutcomePriorityV1"] {
        let listed = listed_in_all(OUTCOME_SOURCE, enum_name);
        for name in &listed {
            let hits = listed.iter().filter(|other| *other == name).count();
            assert_eq!(hits, 1, "{enum_name}: {name} перечислен в ALL {hits} раз");
        }
    }
}

/// Гейт: сам сканер не вакуумен.
///
/// Если бы разбор возвращал пустой список, оба теста выше были бы зелёными на
/// пустых множествах. Тест фиксирует, что сканер действительно читает текст и
/// находит известные имена.
#[test]
fn the_scanner_actually_reads_the_source() {
    let declared = declared_variants(OUTCOME_SOURCE, "QualityOutcomeV1");
    assert_eq!(
        declared.len(),
        15,
        "сканер обязан находить все 15 объявлений"
    );
    assert!(declared.contains(&"Improved".to_string()));
    assert!(declared.contains(&"UnchangedNoAdmittedProfile".to_string()));

    let listed = listed_in_all(OUTCOME_SOURCE, "QualityOutcomeV1");
    assert_eq!(
        listed.len(),
        15,
        "сканер обязан находить все 15 элементов ALL"
    );
    assert_eq!(
        listed.first().map(String::as_str),
        Some("UnchangedImmutable"),
        "ALL обязан начинаться рангом 1"
    );
    assert_eq!(
        listed.last().map(String::as_str),
        Some("Improved"),
        "ALL обязан заканчиваться рангом 15"
    );

    // Удалённый вариант не вправе вернуться незамеченным. Своего имени суб-LSB
    // допуск не требует ни в одном режиме: в `auto` он валит доказательство при
    // сборке, в `lint` то же пустое `L3` штатно сообщается рангом 12
    // (раздел 5.11 контракта).
    assert!(
        !OUTCOME_SOURCE.contains("UnchangedBelowOutputResolution"),
        "исход снят контрактом: в auto ловится при сборке, в lint — ранг 12"
    );
}
