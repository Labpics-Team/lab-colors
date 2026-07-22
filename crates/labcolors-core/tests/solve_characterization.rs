//! Битовая характеризация численного solver-контракта на канонических платформах.
//!
//! Фикстуры `contracts/solve-characterization-v1-{macos-aarch64,linux-x64}.json`
//! обязаны реплеиться бит-в-бит (f64 сравниваются по битам, не по значению)
//! каждая на своей канонической платформе. Rebaseline допустим только вместе с
//! доказанным изменением численного контракта и построчным review diff; любое
//! иное расхождение — дефект PR.
//!
//! Запись эталона текущей платформы (ровно один раз, на baseline):
//! `LABCOLORS_RECORD_SOLVE_CHARACTERIZATION=1 cargo test -p labcolors-core \
//!    --test solve_characterization -- --nocapture`

use std::collections::BTreeMap;
use std::fmt::Write as _;

use labcolors_core::{
    BgInput, ChromaPolicy, Contract, Floor, Gamut, Hue, SolveFailure, SolveJob, Solved,
    ViewingConditions, solve, solve_many,
};

/// Платформенные эталоны. Текущий релиз — `LegacyPlatformDependent` (#297/#292):
/// f64-корреляты CAM16→Oklab проходят через libm (atan2/cbrt), чьи последние
/// ulp расходятся между платформами, поэтому bit-for-bit фикстура пинится ПО
/// ПЛАТФОРМАМ. Обе зафиксированы и обязаны реплеиться бит-в-бит каждая на
/// своей: macos-aarch64 записана локальной канонической машиной; linux-x64 —
/// дословный вывод канонического CI-раннера (реплей PR #327), верифицированный
/// тем же ранером бит-в-бит. Их расхождение задокументировано и запинено тестом
/// `platform_fixtures_agree_except_documented_hue_ulp_drift` ниже — рост дрифта
/// за пределы экспоната = алярм, не «новая платформа шумит».
const FIXTURE_MACOS_AARCH64: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/contracts/solve-characterization-v1-macos-aarch64.json"
);
const FIXTURE_LINUX_X64: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/contracts/solve-characterization-v1-linux-x64.json"
);

/// Эталон текущей платформы. На незапиненной платформе — громкий отказ:
/// характеризация без записанного эталона не «пропускается», её нужно записать.
fn fixture_path() -> &'static str {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        FIXTURE_MACOS_AARCH64
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        FIXTURE_LINUX_X64
    } else {
        panic!(
            "no recorded solve-characterization fixture for this platform; \
             record one with LABCOLORS_RECORD_SOLVE_CHARACTERIZATION=1"
        );
    }
}

/// Битовое представление f64: точность «биты payload не меняются» из #297.
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

#[derive(Debug, Clone, Copy)]
enum FloorSpec {
    Default,
    None,
    AaUi,
}

impl FloorSpec {
    fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::None => "none",
            Self::AaUi => "aa-ui",
        }
    }

    fn apply(self, contract: Contract) -> Contract {
        match self {
            Self::Default => contract,
            Self::None => contract.with_conformance(Floor::None),
            Self::AaUi => contract.with_conformance(Floor::AaUi),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ContractSpec {
    Text(f64),
    Ui(f64),
    Range(f64, f64),
}

impl ContractSpec {
    fn key(self) -> String {
        match self {
            Self::Text(lc) => format!("text({lc})"),
            Self::Ui(lc) => format!("ui({lc})"),
            Self::Range(floor, ceiling) => format!("range({floor},{ceiling})"),
        }
    }

    fn build(self) -> Contract {
        match self {
            Self::Text(lc) => Contract::text(lc),
            Self::Ui(lc) => Contract::ui(lc),
            Self::Range(floor, ceiling) => Contract::range(floor, ceiling),
        }
    }
}

struct CaseSpec {
    bg: &'static str,
    contract: ContractSpec,
    floor: FloorSpec,
    hue: f64,
    chroma: ChromaPolicy,
}

/// Невакуумная матрица: оба знака полярности, floored/non-floored успехи и
/// каждый достижимый класс ошибки. Полоса |Lc| ∈ (7.3, 7.6) — территория
/// квантизационных гэпов (см. `solve.rs` band-scan тест).
fn matrix() -> Vec<CaseSpec> {
    let mut cases = Vec::new();
    let backgrounds = ["#FFFFFF", "#000000", "#767676", "#101012", "#007AFF"];
    let text_targets = [30.0, 60.0, 75.0, 90.0, 150.0, -30.0, -60.0, -75.0, -90.0];
    for bg in backgrounds {
        for target in text_targets {
            cases.push(CaseSpec {
                bg,
                contract: ContractSpec::Text(target),
                floor: FloorSpec::Default,
                hue: 264.0,
                chroma: ChromaPolicy::Neutral,
            });
        }
    }
    // Квантизационная полоса: мелкий шаг у нижней границы применимости LPC.
    let mut t = 7.30_f64;
    while t <= 7.60 + 1e-9 {
        cases.push(CaseSpec {
            bg: "#FFFFFF",
            contract: ContractSpec::Text(t),
            floor: FloorSpec::None,
            hue: 0.0,
            chroma: ChromaPolicy::Neutral,
        });
        cases.push(CaseSpec {
            bg: "#000000",
            contract: ContractSpec::Text(-t),
            floor: FloorSpec::None,
            hue: 0.0,
            chroma: ChromaPolicy::Neutral,
        });
        t += 0.05;
    }
    // Средне-серые фоны: территория FloorUnreachable для dark-on-light AA.
    for bg in ["#6E6E6E", "#7A7A7A", "#828282"] {
        for target in [20.0, 35.0, 45.0] {
            cases.push(CaseSpec {
                bg,
                contract: ContractSpec::Text(target),
                floor: FloorSpec::Default,
                hue: 145.0,
                chroma: ChromaPolicy::Relative(0.35),
            });
        }
    }
    // UI и range контракты, обе полярности, вариации floor-спеки.
    for (bg, target) in [("#FFFFFF", 45.0), ("#101012", -45.0)] {
        cases.push(CaseSpec {
            bg,
            contract: ContractSpec::Ui(target),
            floor: FloorSpec::Default,
            hue: 30.0,
            chroma: ChromaPolicy::Relative(0.6),
        });
        cases.push(CaseSpec {
            bg,
            contract: ContractSpec::Ui(target),
            floor: FloorSpec::AaUi,
            hue: 30.0,
            chroma: ChromaPolicy::Neutral,
        });
    }
    for (bg, floor, ceiling) in [("#FFFFFF", 12.0, 20.0), ("#000000", -20.0, -12.0)] {
        cases.push(CaseSpec {
            bg,
            contract: ContractSpec::Range(floor, ceiling),
            floor: FloorSpec::Default,
            hue: 200.0,
            chroma: ChromaPolicy::Relative(0.2),
        });
    }
    // Заведомо мёртвая зона и невалидный вход.
    cases.push(CaseSpec {
        bg: "#FFFFFF",
        contract: ContractSpec::Text(3.0),
        floor: FloorSpec::None,
        hue: 0.0,
        chroma: ChromaPolicy::Neutral,
    });
    cases.push(CaseSpec {
        bg: "not-a-color",
        contract: ContractSpec::Text(60.0),
        floor: FloorSpec::Default,
        hue: 0.0,
        chroma: ChromaPolicy::Neutral,
    });
    cases
}

fn chroma_key(policy: ChromaPolicy) -> String {
    match policy {
        ChromaPolicy::Neutral => "neutral".to_string(),
        ChromaPolicy::Relative(fraction) => format!("relative({fraction})"),
    }
}

fn case_key(case: &CaseSpec) -> String {
    format!(
        "bg={} contract={} floor={} hue={} chroma={}",
        case.bg,
        case.contract.key(),
        case.floor.key(),
        case.hue,
        chroma_key(case.chroma),
    )
}

/// Точная сериализация исхода: hex-байты + битовые f64 + все поля ошибок.
fn outcome_line(result: &Result<Solved, SolveFailure>) -> String {
    match result {
        Ok(solved) => format!(
            "ok hex={} lc_bits={} wcag_ratio_bits={} floor_override={} jp_bits={} h_ok_bits={} s_bits={}",
            solved.hex(),
            bits(solved.lc()),
            bits(solved.wcag_ratio()),
            solved.floor_override(),
            bits(solved.color().jp()),
            bits(solved.color().h_ok()),
            bits(solved.color().s()),
        ),
        Err(SolveFailure::BelowContrastFloor { target }) => {
            format!("err below_contrast_floor target_bits={}", bits(*target))
        }
        Err(SolveFailure::ExceedsRange {
            target,
            max_achievable,
        }) => format!(
            "err exceeds_range target_bits={} max_achievable_bits={}",
            bits(*target),
            bits(*max_achievable)
        ),
        Err(SolveFailure::BoundedSearchExhausted {
            target,
            closest_examined,
        }) => format!(
            "err bounded_search_exhausted target_bits={} closest_examined_bits={}",
            bits(*target),
            bits(*closest_examined)
        ),
        Err(SolveFailure::FloorUnreachable { floor, max_ratio }) => format!(
            "err floor_unreachable floor_bits={} max_ratio_bits={}",
            bits(*floor),
            bits(*max_ratio)
        ),
        Err(SolveFailure::InvalidInput(message)) => {
            format!("err invalid_input message={message:?}")
        }
        Err(SolveFailure::InternalInvariant(message)) => {
            format!("err internal_invariant message={message:?}")
        }
        Err(other) => format!("err unknown {other:?}"),
    }
}

fn run_case(case: &CaseSpec) -> Result<Solved, SolveFailure> {
    let bg = BgInput::solid(case.bg)?;
    solve(
        bg,
        case.floor.apply(case.contract.build()),
        Hue::deg(case.hue),
        case.chroma,
        &ViewingConditions::srgb(),
        Gamut::Srgb,
    )
}

fn observed_map() -> BTreeMap<String, String> {
    let mut observed = BTreeMap::new();
    for case in matrix() {
        let previous = observed.insert(case_key(&case), outcome_line(&run_case(&case)));
        assert!(
            previous.is_none(),
            "duplicate case key: {}",
            case_key(&case)
        );
    }
    observed
}

fn render(observed: &BTreeMap<String, String>) -> String {
    let mut out = String::from("{\n");
    let mut first = true;
    for (key, value) in observed {
        if !first {
            out.push_str(",\n");
        }
        first = false;
        write!(out, "  {}: {}", json_string(key), json_string(value)).unwrap();
    }
    out.push_str("\n}\n");
    out
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            control if (control as u32) < 0x20 => write!(out, "\\u{:04x}", control as u32).unwrap(),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[test]
fn fixture_replays_bit_for_bit() {
    let observed = observed_map();
    let rendered = render(&observed);
    let fixture = fixture_path();
    if std::env::var_os("LABCOLORS_RECORD_SOLVE_CHARACTERIZATION").is_some() {
        // Дисциплина append-only рекордера:
        // перезапись закоммиченного эталона — только осознанным `rm` заранее,
        // и recorder-ран НИКОГДА не зелёный, чтобы случайный запуск с env-var
        // не мог превратить численный регресс в прошедший тест.
        assert!(
            !std::path::Path::new(fixture).exists(),
            "refusing to overwrite the committed fixture {fixture}; \
             rebaseline must be a deliberate act — delete the file first"
        );
        std::fs::write(fixture, &rendered).expect("fixture written");
        panic!(
            "solve characterization recorded: {} cases -> {fixture}; \
             recording runs never pass — rerun without \
             LABCOLORS_RECORD_SOLVE_CHARACTERIZATION to verify the replay",
            observed.len()
        );
    }
    let committed =
        std::fs::read_to_string(fixture).expect("committed solve characterization fixture exists");
    assert_eq!(
        rendered, committed,
        "solve characterization drifted from its reviewed baseline; \
         numeric changes require an explicit recorder diff and proof"
    );
}

/// Анти-вакуум: матрица обязана населять оба знака, floored/non-floored успехи
/// и каждый достижимый класс ошибки.
#[test]
fn characterization_counters_are_non_vacuous() {
    let observed = observed_map();
    let mut successes = 0_usize;
    let mut floored = 0_usize;
    let mut unfloored = 0_usize;
    let mut positive = 0_usize;
    let mut negative = 0_usize;
    let mut class_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (key, line) in &observed {
        if line.starts_with("ok ") {
            successes += 1;
            if line.contains("floor_override=true") {
                floored += 1;
            } else {
                unfloored += 1;
            }
            if key.contains("contract=text(-")
                || key.contains("contract=ui(-")
                || key.contains("contract=range(-")
            {
                negative += 1;
            } else {
                positive += 1;
            }
        } else {
            let class = line
                .strip_prefix("err ")
                .and_then(|rest| rest.split_whitespace().next())
                .expect("error line carries a class");
            let slot = match class {
                "below_contrast_floor" => "below_contrast_floor",
                "exceeds_range" => "exceeds_range",
                "bounded_search_exhausted" => "bounded_search_exhausted",
                "floor_unreachable" => "floor_unreachable",
                "invalid_input" => "invalid_input",
                "internal_invariant" => "internal_invariant",
                other => panic!("unknown error class {other}"),
            };
            *class_counts.entry(slot).or_default() += 1;
        }
    }
    assert!(successes >= 10, "successes: {successes}");
    assert!(floored >= 2, "floored successes: {floored}");
    assert!(unfloored >= 2, "unfloored successes: {unfloored}");
    assert!(positive >= 3 && negative >= 3, "+{positive}/-{negative}");
    for class in [
        "below_contrast_floor",
        "exceeds_range",
        "floor_unreachable",
        "invalid_input",
    ] {
        assert!(
            class_counts.get(class).copied().unwrap_or(0) >= 1,
            "error class {class} is not populated; counts: {class_counts:?}"
        );
    }
    // BoundedSearchExhausted на публичной поверхности ВЫМЕР. Структурно: допуск
    // `meets_floor_lc` (−1 Lc) вместе с QUANT_BUDGET=1 даёт окно приёмки в
    // 2 Lc, а same-polarity окна сетки шире 2 Lc существуют только вплотную к
    // аналитическому клипу, где отказ принадлежит BelowContrastFloor ЕЩЁ ДО
    // квантования; walk в 2 distinct-шага пересекает всё остальное (фикс #44).
    // Эмпирически: сканы публичного API на миллионы вызовов (solid-фоны обеих
    // полярностей, серые и хроматические, hue-сетка, Neutral/Relative вплоть до
    // 1.0, Floor::None/AaText/AaUi, |Lc| 7.3..112, srgb и dim surround) не
    // производят ни одного. Правда самого варианта (`closest_examined` локален,
    // не глобален) запинена на его собственном шве:
    // `solve::tests::bounded_search_exhausted_is_local_not_global_counterexample`.
    // Появление исхода из этой матрицы = изменение поведения поиска, не «новый кейс».
    assert_eq!(
        class_counts
            .get("bounded_search_exhausted")
            .copied()
            .unwrap_or(0),
        0,
        "BoundedSearchExhausted is characterized as publicly extinct on this matrix"
    );
    assert_eq!(
        class_counts.get("internal_invariant").copied().unwrap_or(0),
        0,
        "characterization inputs must not trip internal invariants"
    );
}

/// `solve_many(bg, jobs) == jobs.map(solve)` позиционно: успехи, каждый класс
/// per-job ошибки, пустой вход, дубликаты и смешанные валидные/невалидные
/// задания.
#[test]
fn solve_many_is_positionally_identical_to_sequential_solve() {
    let vc = ViewingConditions::srgb();
    let job = |contract: Contract, hue: f64, chroma: ChromaPolicy| SolveJob {
        contract,
        hue: Hue::deg(hue),
        chroma_policy: chroma,
    };
    let jobs = vec![
        job(Contract::text(60.0), 264.0, ChromaPolicy::Neutral),
        job(Contract::text(150.0), 0.0, ChromaPolicy::Neutral),
        job(
            Contract::text(7.45).with_conformance(Floor::None),
            0.0,
            ChromaPolicy::Neutral,
        ),
        job(
            Contract::text(3.0).with_conformance(Floor::None),
            0.0,
            ChromaPolicy::Neutral,
        ),
        job(Contract::ui(45.0), 30.0, ChromaPolicy::Relative(0.6)),
        // Дубликат первого задания: позиционность, не дедупликация.
        job(Contract::text(60.0), 264.0, ChromaPolicy::Neutral),
        job(
            Contract::range(12.0, 20.0),
            200.0,
            ChromaPolicy::Relative(0.2),
        ),
        // Смешанный batch: невалидное per-job задание (chroma-ratio вне [0,1])
        // между валидными — обязано стать позиционным Err, не сдвинуть соседей
        // и не уронить партию (требование #297 «mixed valid/invalid jobs»).
        job(Contract::text(60.0), 264.0, ChromaPolicy::Relative(-0.25)),
        job(Contract::text(45.0), 90.0, ChromaPolicy::Relative(0.3)),
    ];
    let bg = BgInput::solid("#FFFFFF").expect("literal background");
    let batch = solve_many(bg, &jobs, &vc, Gamut::Srgb).expect("batch runs");
    assert_eq!(batch.len(), jobs.len());

    let mut ok = 0_usize;
    let mut err_classes: BTreeMap<String, usize> = BTreeMap::new();
    for (index, job) in jobs.iter().enumerate() {
        let bg = BgInput::solid("#FFFFFF").expect("literal background");
        let sequential = solve(
            bg,
            job.contract,
            job.hue,
            job.chroma_policy,
            &vc,
            Gamut::Srgb,
        );
        assert_eq!(
            outcome_line(&batch[index]),
            outcome_line(&sequential),
            "position {index} diverged"
        );
        match &batch[index] {
            Ok(_) => ok += 1,
            Err(error) => {
                let line = outcome_line(&Err::<Solved, _>(error.clone()));
                let class = line
                    .strip_prefix("err ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .expect("class")
                    .to_string();
                *err_classes.entry(class).or_default() += 1;
            }
        }
    }
    // Анти-вакуум партии: успехи, дубликат успеха и ≥3 разных класса per-job
    // ошибок (включая invalid_input от смешанного задания); партия целиком из
    // Err пройти не может.
    assert!(ok >= 4, "batch successes: {ok}");
    assert!(
        err_classes.len() >= 3,
        "batch must exercise several per-job error classes: {err_classes:?}"
    );
    assert!(
        err_classes.contains_key("invalid_input"),
        "the mixed batch must carry a positional invalid job: {err_classes:?}"
    );
    assert_eq!(
        outcome_line(&batch[0]),
        outcome_line(&batch[5]),
        "duplicate jobs"
    );
    assert!(
        batch[7].is_err() && batch[8].is_ok(),
        "the invalid job must not shift or poison its valid neighbour"
    );

    // Отдельная партия на средне-сером #6E6E6E: dark-on-light AA-text там
    // математически не достигает 4.5:1 (потолок ~4.14), поэтому per-job
    // FloorUnreachable обязан быть позиционным исходом и совпадать с
    // последовательным solve.
    let grey_jobs = vec![
        job(Contract::text(20.0), 145.0, ChromaPolicy::Neutral),
        job(
            Contract::text(9.0).with_conformance(Floor::None),
            0.0,
            ChromaPolicy::Neutral,
        ),
    ];
    let grey_bg = BgInput::solid("#6E6E6E").expect("literal background");
    let grey_batch = solve_many(grey_bg, &grey_jobs, &vc, Gamut::Srgb).expect("batch runs");
    for (index, grey_job) in grey_jobs.iter().enumerate() {
        let bg = BgInput::solid("#6E6E6E").expect("literal background");
        let sequential = solve(
            bg,
            grey_job.contract,
            grey_job.hue,
            grey_job.chroma_policy,
            &vc,
            Gamut::Srgb,
        );
        assert_eq!(
            outcome_line(&grey_batch[index]),
            outcome_line(&sequential),
            "grey position {index} diverged"
        );
    }
    assert!(
        outcome_line(&grey_batch[0]).starts_with("err floor_unreachable"),
        "mid-grey AA-text job must be a positional FloorUnreachable: {}",
        outcome_line(&grey_batch[0])
    );
    assert!(grey_batch[1].is_ok(), "the floorless grey job must resolve");

    // Пустой вход — пустой результат.
    let bg = BgInput::solid("#FFFFFF").expect("literal background");
    assert!(
        solve_many(bg, &[], &vc, Gamut::Srgb)
            .expect("empty batch")
            .is_empty()
    );
}

/// Позитивная характеризация JND-полосы против `recheck_against` — публичного
/// пути перемера, которым адаптивный рантайм проверяет цвета каждый кадр. Это
/// независимый ПУТЬ (другой вход, другая горячая экономия), но та же
/// измерительная сердцевина `lpc::contrast_core` — тест пинит согласованность
/// оси читаемости между solve и recheck, не независимый вывод самой метрики.
/// Пинится наблюдаемый контракт локального поиска:
///
/// 1. полоса в основном разрешается (анти-вакуум: all-Err пройти не может);
/// 2. каждый разрешённый цвет попадает в симметричный бюджет ±1 Lc;
/// 3. репортуемый `lc` бит-в-бит равен независимому перемеру того же hex —
///    измерительная честность: `finish` и `recheck_against` читают одну ось;
/// 4. приёмка ТОЛЕРАНТНА: существуют разрешённые случаи, где достигнутый `lc`
///    строго НЕ дотягивает до цели (в пределах нижнего допуска бюджета) — то
///    есть «решено» на этой поверхности значит «в допуске», а не «на-или-за
///    целью». Это зафиксированное текущее поведение, которое честные имена
///    #297 обязаны проговорить, а не спрятать.
///
/// Ни один вход полосы не смеет выносить BoundedSearchExhausted (публичное
/// вымирание — см. counters-тест); локальность reported near-miss
/// запинена контрпримерами на шве поиска (unit-тесты solve.rs:
/// `bounded_search_exhausted_is_local_not_global_counterexample`,
/// `dj_degraded_selection_is_local_not_global`).
#[test]
fn jnd_band_resolves_within_budget_with_tolerant_acceptance() {
    let vc = ViewingConditions::srgb();
    let mut tolerated_undershoot = 0_usize;
    for (bg_hex, pol) in [("#FFFFFF", 1.0_f64), ("#000000", -1.0_f64)] {
        let mut resolved = 0_usize;
        let mut t = 7.30_f64;
        while t <= 7.60 + 1e-9 {
            let target = t * pol;
            let bg = BgInput::solid(bg_hex).expect("literal background");
            let result = solve(
                bg,
                Contract::text(target).with_conformance(Floor::None),
                Hue::deg(0.0),
                ChromaPolicy::Neutral,
                &vc,
                Gamut::Srgb,
            );
            match result {
                Ok(solved) => {
                    resolved += 1;
                    assert!(
                        (solved.lc() - target).abs() <= 1.0 + 1e-12,
                        "{bg_hex} {target}: resolved lc {} escapes the ±1 budget",
                        solved.lc()
                    );
                    let remeasured = labcolors_core::recheck_against(bg_hex, &[solved.hex()], &vc)
                        .expect("emitted hex rechecks");
                    assert_eq!(
                        bits(solved.lc()),
                        bits(remeasured[0].0),
                        "{bg_hex} {target}: reported lc diverges from the independent \
                         re-measurement of the same hex"
                    );
                    let undershoots = if pol >= 0.0 {
                        solved.lc() < target
                    } else {
                        solved.lc() > target
                    };
                    if undershoots {
                        tolerated_undershoot += 1;
                    }
                }
                Err(SolveFailure::BelowContrastFloor { .. }) => {}
                Err(other) => panic!(
                    "{bg_hex} {target}: band may refuse only via the analytic dead \
                     zone; got {other:?}"
                ),
            }
            t += 0.01;
        }
        assert!(
            resolved >= 20,
            "anti-vacuum: the JND band on {bg_hex} must mostly resolve; got {resolved}"
        );
    }
    // Толерантная нижняя приёмка обязана реально стрелять хотя бы на одном
    // фоне полосы (сегодня — на #000000: цель −7.36 принимает #323232 с
    // lc −7.3502, недолёт 0.0098 внутри допуска).
    assert!(
        tolerated_undershoot >= 1,
        "anti-vacuum: the tolerant lower acceptance never fired on the band"
    );
}

/// Экспонат платформенной зависимости текущего релиза (#297 «current path is
/// LegacyPlatformDependent»): между канонической macOS-arm64 и каноническим
/// Linux-x64 расходится РОВНО хвост ulp одного поля — Oklab-hue коррелята
/// `h_ok` — в двух хроматических кейсах матрицы. Всё остальное (hex-байты, `lc`,
/// `wcag_ratio`, `floor_override`, `jp`, `s`, все payload'ы ошибок) —
/// бит-идентично на всех 76 кейсах. Оба кейса — libm-разница
/// (atan2/cbrt, 5 ulp на хроматике). У точного sRGB-серого `h_ok = 0` по
/// определению, поэтому его прежний atan2-шум больше не является частью
/// платформенного контракта. Рост этого множества — изменение численного
/// поведения, а не «шум новой платформы».
#[test]
fn platform_fixtures_agree_except_documented_hue_ulp_drift() {
    let load = |path: &str| -> BTreeMap<String, String> {
        let text = std::fs::read_to_string(path).expect("committed fixture exists");
        let mut out = BTreeMap::new();
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix('"') else {
                continue;
            };
            let Some((key, value_part)) = rest.split_once("\": \"") else {
                continue;
            };
            let value = value_part
                .trim_end_matches(',')
                .trim_end_matches('"')
                .to_string();
            out.insert(key.to_string(), value);
        }
        out
    };
    let mac = load(FIXTURE_MACOS_AARCH64);
    let linux = load(FIXTURE_LINUX_X64);
    assert_eq!(mac.len(), 76, "macOS fixture cardinality");
    assert_eq!(
        mac.keys().collect::<Vec<_>>(),
        linux.keys().collect::<Vec<_>>(),
        "the two platform fixtures must pin the same case matrix"
    );

    let mut drifted: Vec<(String, Vec<String>)> = Vec::new();
    for (key, mac_line) in &mac {
        let linux_line = &linux[key];
        if mac_line == linux_line {
            continue;
        }
        let fields = |line: &str| -> BTreeMap<String, String> {
            line.strip_prefix("ok ")
                .unwrap_or(line)
                .split_whitespace()
                .filter_map(|pair| pair.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let mf = fields(mac_line);
        let lf = fields(linux_line);
        let differing: Vec<String> = mf
            .iter()
            .filter(|(k, v)| lf.get(*k) != Some(v))
            .map(|(k, _)| k.clone())
            .collect();
        drifted.push((key.clone(), differing));
    }
    drifted.sort();

    let expected: Vec<(String, Vec<String>)> = vec![
        (
            "bg=#7A7A7A contract=text(20) floor=default hue=145 chroma=relative(0.35)".to_string(),
            vec!["h_ok_bits".to_string()],
        ),
        (
            "bg=#7A7A7A contract=text(35) floor=default hue=145 chroma=relative(0.35)".to_string(),
            vec!["h_ok_bits".to_string()],
        ),
    ];
    assert_eq!(
        drifted, expected,
        "the cross-platform drift exhibit must stay exactly the documented \
         two chromatic h_ok ulp-tail cases"
    );
}
