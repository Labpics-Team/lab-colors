//! Figma-щит вокабуляра (канон владельца, см. `docs/vocabulary.md`).
//!
//! Снапшот `tools/figma-vocab/semantic.snapshot.txt` — экспорт имён переменных
//! коллекции «🔵 4.2 Semantic» (VariableCollectionId:4001:165) файла
//! «🧪Lab UI (v.1)» (LuaiBd4anRi4DMZayKAnY2).
//!
//! Канон: ровно пять семей. Любое имя вне грамматики — красный CI:
//! сначала решение владельца, потом имя. Список [`GRANDFATHERED`] только
//! уменьшается — расширять его запрещено.

const FAMILIES: [&str; 5] = ["Backgrounds/", "Fills/", "Labels/", "Border/", "FX/"];

/// Легаси вне канона; живёт до растворения Misc (таск «Misc-маппинг»).
/// Добавлять сюда НЕЛЬЗЯ — только удалять по мере растворения.
const GRANDFATHERED: [&str; 3] = [
    "Misc/Badge/Label-contrast",
    "Misc/Badge/Label-default",
    "Misc/Control/Control-bg",
];

const SNAPSHOT: &str = include_str!("../../../tools/figma-vocab/semantic.snapshot.txt");

fn names() -> impl Iterator<Item = &'static str> {
    SNAPSHOT
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

#[test]
fn every_variable_belongs_to_a_canon_family_or_is_grandfathered() {
    let violations: Vec<_> = names()
        .filter(|n| {
            let canon = FAMILIES.iter().any(|f| n.starts_with(f));
            let legacy = GRANDFATHERED.contains(n);
            !(canon || legacy)
        })
        .collect();
    assert!(
        violations.is_empty(),
        "имена вне канона (пять семей + grandfather): {violations:?}"
    );
}

#[test]
fn names_are_clean() {
    for name in names() {
        assert!(
            !name.starts_with('/') && !name.ends_with('/'),
            "висячий слэш: {name}"
        );
        assert!(!name.contains(':'), "двоеточие в имени: {name}");
        assert!(!name.contains("//"), "пустой сегмент: {name}");
        for seg in name.split('/') {
            assert!(
                seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
                "недопустимые символы в сегменте {seg:?} имени {name}"
            );
        }
    }
}

#[test]
fn grandfather_list_only_shrinks() {
    // Каждая запись обязана ещё существовать в снапшоте: если её растворили,
    // её надо удалить и отсюда — список не копит мёртвые записи.
    for g in GRANDFATHERED {
        assert!(
            names().any(|n| n == g),
            "решён и должен быть удалён из GRANDFATHERED: {g}"
        );
    }
}

#[test]
fn snapshot_is_sorted_and_unique() {
    let all: Vec<_> = names().collect();
    let mut sorted = all.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(all, sorted, "снапшот обязан быть отсортирован и без дублей");
}
