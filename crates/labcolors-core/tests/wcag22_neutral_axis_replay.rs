//! Replay нейтральной оси против независимого exact-оракула (#295).
//!
//! Оракул `scripts/verify_wcag22_neutral_axis.py` пересчитывает те же множества
//! рациональной арифметикой без Q55 и без Rust-вычислителя. Его закоммиченный
//! артефакт запинен здесь по SHA-256; таблица сценариев — единый источник и
//! для побайтовой привязки к артефакту, и для replay через ПУБЛИЧНЫЙ
//! exact-вычислитель `evaluate_wcag22_srgb8`. Изменение соседей, критерия или
//! домена меняет множество решений и требует пересчёта оракула.

// Включение по #[path] компилирует модуль заново в этом крейте: replay
// использует только digest/to_hex, остальная поверхность здесь не нужна.
#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod fixture_sha256;

use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, evaluate_wcag22_srgb8,
};

fn oracle_fixture() -> String {
    include_str!("../contracts/wcag22-neutral-axis-oracle-v1.json").replace("\r\n", "\n")
}

/// Один сценарий оракула. Поля зеркалят fixture-объект артефакта; порядок
/// соседей и критериев — как в его canonical JSON.
struct Scenario {
    id: &'static str,
    adjacent: &'static [u8],
    criteria: &'static [&'static str],
    /// Инклюзивные серые диапазоны решения, как в `candidate_ranges` оракула.
    ranges: &'static [(u8, u8)],
}

const SCENARIOS: [Scenario; 5] = [
    Scenario {
        id: "normal-text-vs-767676",
        adjacent: &[0x76],
        criteria: &["sc-1.4.3-text-default"],
        ranges: &[(0x00, 0x04), (0xFE, 0xFF)],
    },
    Scenario {
        id: "normal-text-vs-black-white",
        adjacent: &[0x00, 0xFF],
        criteria: &["sc-1.4.3-text-default"],
        ranges: &[(0x75, 0x76)],
    },
    Scenario {
        id: "normal-text-vs-black-white-767676",
        adjacent: &[0x00, 0x76, 0xFF],
        criteria: &["sc-1.4.3-text-default"],
        ranges: &[],
    },
    Scenario {
        id: "three-to-one-vs-767676",
        adjacent: &[0x76],
        criteria: &[
            "sc-1.4.3-text-large-scale",
            "sc-1.4.11-ui-component-or-state",
            "sc-1.4.11-graphical-object",
        ],
        ranges: &[(0x00, 0x2D), (0xD2, 0xFF)],
    },
    Scenario {
        id: "three-to-one-vs-black-white",
        adjacent: &[0x00, 0xFF],
        criteria: &[
            "sc-1.4.3-text-large-scale",
            "sc-1.4.11-ui-component-or-state",
            "sc-1.4.11-graphical-object",
        ],
        ranges: &[(0x5A, 0x94)],
    },
];

fn grey_hex(value: u8) -> String {
    format!("#{value:02X}{value:02X}{value:02X}")
}

fn json_string_list(items: impl Iterator<Item = String>) -> String {
    items
        .map(|item| format!("\"{item}\""))
        .collect::<Vec<_>>()
        .join(",")
}

fn expected_solution(scenario: &Scenario) -> Vec<u8> {
    scenario
        .ranges
        .iter()
        .flat_map(|&(start, end)| start..=end)
        .collect()
}

fn parsed_criteria(scenario: &Scenario) -> Vec<Wcag22CriterionV1> {
    scenario
        .criteria
        .iter()
        .map(|key| {
            Wcag22CriterionV1::parse(key)
                .unwrap_or_else(|| panic!("oracle criterion key must stay public: {key}"))
        })
        .collect()
}

fn passes(candidate: [u8; 3], adjacent: [u8; 3], criterion: Wcag22CriterionV1) -> bool {
    match evaluate_wcag22_srgb8(candidate, adjacent, criterion) {
        Ok(Wcag22AssessmentV1::Evaluated { decision, .. }) => {
            decision == Wcag22ApplicableDecisionV1::Pass
        }
        other => panic!("evaluator must stay total on byte input: {other:?}"),
    }
}

/// Полное решение оси для набора критериев: серые `v`, проходящие КАЖДОГО
/// соседа по КАЖДОМУ критерию набора.
fn replay_solution(adjacent: &[u8], criteria: &[Wcag22CriterionV1]) -> Vec<u8> {
    (0_u16..=255)
        .map(|v| v as u8)
        .filter(|v| {
            adjacent.iter().all(|n| {
                criteria
                    .iter()
                    .all(|criterion| passes([*v; 3], [*n; 3], *criterion))
            })
        })
        .collect()
}

#[test]
fn production_replay_is_bound_to_the_exact_independent_oracle_fixture() {
    assert_eq!(
        fixture_sha256::digest(oracle_fixture().as_bytes()).to_hex(),
        "af56e71febf2994a186a7d4b1e51d5297263220f4adbe482d8c7a7f3b155f8b2"
    );
}

/// Таблица сценариев не имеет права разойтись с содержимым артефакта: каждый
/// сценарий обязан существовать в оракуле с ровно этими соседями, мощностью,
/// диапазонами, критериями и id. Canonical JSON компактен, ключи объекта
/// отсортированы (`adjacent`, `candidate_count`, `candidate_ranges`,
/// `candidate_set_sha256`, `criteria`, `id`) — между head и tail одного
/// объекта обязан лежать ровно его `candidate_set_sha256`.
#[test]
fn scenario_table_is_bound_to_the_oracle_fixture_bytes() {
    for scenario in &SCENARIOS {
        let ranges = scenario
            .ranges
            .iter()
            .map(|&(start, end)| format!("[\"{}\",\"{}\"]", grey_hex(start), grey_hex(end)))
            .collect::<Vec<_>>()
            .join(",");
        let head = format!(
            "{{\"adjacent\":[{}],\"candidate_count\":{},\"candidate_ranges\":[{ranges}],",
            json_string_list(scenario.adjacent.iter().map(|&value| grey_hex(value))),
            expected_solution(scenario).len(),
        );
        let tail = format!(
            "\"criteria\":[{}],\"id\":\"{}\"}}",
            json_string_list(scenario.criteria.iter().map(|key| (*key).to_owned())),
            scenario.id,
        );

        let head_at = oracle_fixture()
            .find(&head)
            .unwrap_or_else(|| panic!("{}: oracle fixture lacks fragment {head}", scenario.id));
        let between_at = head_at + head.len();
        let tail_at = oracle_fixture()[between_at..]
            .find(&tail)
            .map(|offset| between_at + offset)
            .unwrap_or_else(|| panic!("{}: oracle fixture lacks fragment {tail}", scenario.id));

        let between = &oracle_fixture()[between_at..tail_at];
        let digest_value = between
            .strip_prefix("\"candidate_set_sha256\":\"")
            .and_then(|rest| rest.strip_suffix("\","))
            .unwrap_or_else(|| {
                panic!(
                    "{}: head and tail must bound one object: {between}",
                    scenario.id
                )
            });
        assert!(
            digest_value.len() == 64
                && digest_value
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{}: candidate_set_sha256 must be one lowercase SHA-256",
            scenario.id,
        );
    }
}

#[test]
fn production_evaluator_replays_every_oracle_scenario() {
    for scenario in &SCENARIOS {
        let criteria = parsed_criteria(scenario);
        let expected = expected_solution(scenario);
        assert_eq!(
            replay_solution(scenario.adjacent, &criteria),
            expected,
            "{}",
            scenario.id
        );
        // Оракул группирует критерии с совпадающим нормативным отношением;
        // производственный вычислитель обязан давать то же множество и для
        // каждого критерия поодиночке, не только для пересечения набора.
        for criterion in criteria {
            assert_eq!(
                replay_solution(scenario.adjacent, &[criterion]),
                expected,
                "{}: {criterion:?}",
                scenario.id
            );
        }
    }
}
