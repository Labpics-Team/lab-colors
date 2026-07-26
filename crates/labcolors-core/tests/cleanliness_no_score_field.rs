//! Гейт: ни один тип модуля `cleanliness` не несёт числового поля.
//!
//! КЛАСС БАГА: **мера грязности заводится полем структуры**, а не методом.
//!
//! Раздел 4 контракта запрещает единый score: величины «насколько цвет грязный»
//! не существует, потому что объявленные популяции расходятся, а усреднение
//! подменило бы предмет спора. Раздел 4 прямо называет `cleanliness_score`
//! отсутствующим в API.
//!
//! `compile_fail`-банк в `lib.rs` щупает четыре правдоподобных **имени метода**
//! на одном типе. Этого мало: имя может быть пятым, а поле — вообще не метод.
//!
//! ГРАНИЦА ГЕЙТА. Этот текст дословно повторён в двух местах — в шапке
//! модуля `cleanliness` и в шапке гейта `cleanliness_no_score_field`; их
//! совпадение проверяет тест `both_boundary_texts_are_identical`.
//!
//! Гейт читает файлы модуля как текст и ловит **прямое объявление** числа:
//!
//! * поле в фигурных скобках и в кортежной форме, при любой видимости;
//! * псевдоним модуля, разворачиваемый транзитивно, включая записанный путём
//!   (`core::primitive::u8`) и параметризованный (`Carrier<f64>`);
//! * макро-вызов уровня элемента, содержащий число;
//! * **любой файл модуля**: каталог обходится, и файл, добавленный завтра,
//!   попадает в проверку сам.
//!
//! Чего он **не** ловит: число, спрятанное в тип, объявленный вне модуля
//! `cleanliness`, и любую индирекцию, которую текстовый разбор не разворачивает.
//!
//! Гейт останавливает **случайное** введение меры, а барьером против
//! намеренного обхода не является и им не притворяется: текстовый сканер таким
//! барьером быть не может.
//!
//! Нулевых зависимостей: чистый `std`, как и прочие сканеры крейта.

mod common;

use common::src_dir;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// Числовые примитивы, недопустимые в позиции поля.
const NUMERIC: [&str; 14] = [
    "f32", "f64", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
    "isize",
];

/// Все файлы модуля: `cleanliness.rs` плюс каждый `*.rs` в `cleanliness/`.
///
/// Каталог обходится намеренно. Жёстко перечисленный список файлов обходился бы
/// добавлением шестого файла — это не гипотеза, а найденный в ревью обход.
fn module_sources() -> Vec<(String, String)> {
    let root: PathBuf = src_dir();
    let mut files = vec![root.join("cleanliness.rs")];

    let dir = root.join("cleanliness");
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("не читается каталог модуля {}: {e}", dir.display()));
    let mut nested: Vec<PathBuf> = entries
        .map(|entry| entry.expect("запись каталога читается").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .collect();
    nested.sort();
    files.extend(nested);

    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("не читается {}: {e}", path.display()));
            let name = path
                .file_name()
                .expect("у файла есть имя")
                .to_string_lossy()
                .into_owned();
            (name, text)
        })
        .collect()
}

/// Последний сегмент пути типа: `core::primitive::u8` → `u8`.
///
/// Без нормализации псевдоним, записанный путём, не разворачивался — тоже
/// найденный в ревью обход, а не предосторожность на всякий случай.
fn last_segment(type_path: &str) -> &str {
    type_path.rsplit("::").next().unwrap_or(type_path).trim()
}

/// Псевдонимы модуля, раскрывающиеся в число, до неподвижной точки.
fn numeric_aliases(sources: &[(String, String)]) -> BTreeSet<String> {
    let mut known: BTreeSet<String> = NUMERIC.iter().map(|n| (*n).to_string()).collect();
    let mut grew = true;

    while grew {
        grew = false;
        for (_, source) in sources {
            for line in source.lines() {
                let line = line.trim();
                let Some(rest) = line
                    .strip_prefix("pub type ")
                    .or_else(|| line.strip_prefix("type "))
                else {
                    continue;
                };
                let Some((name, target)) = rest.split_once('=') else {
                    continue;
                };
                let target = last_segment(target.trim().trim_end_matches(';'));
                if known.contains(target) && known.insert(name.trim().to_string()) {
                    grew = true;
                }
            }
        }
    }
    known
}

/// Начинается ли строка объявлением типа при любой видимости.
fn declares_type(line: &str) -> bool {
    let rest = line
        .strip_prefix("pub(crate) ")
        .or_else(|| line.strip_prefix("pub(super) "))
        .or_else(|| line.strip_prefix("pub "))
        .unwrap_or(line);
    rest.starts_with("struct ") || rest.starts_with("enum ")
}

/// Строки в позиции поля: тела в фигурных скобках и кортежные объявления.
fn field_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut depth = 0usize;

    for (number, raw) in source.lines().enumerate() {
        let line = raw.trim();

        if depth > 0 {
            if line.starts_with('}') {
                depth -= 1;
                continue;
            }
            if !line.is_empty() && !line.starts_with("//") && !line.starts_with('#') {
                out.push((number + 1, line.to_string()));
            }
            continue;
        }

        if !declares_type(line) {
            continue;
        }
        if line.ends_with('{') {
            depth += 1;
        } else if let Some(open) = line.find('(') {
            let tuple = line[open + 1..].trim_end_matches([';', ')']);
            out.push((number + 1, tuple.to_string()));
        }
    }
    out
}

/// Встречается ли одно из имён отдельным токеном либо последним сегментом пути.
fn mentions_any(line: &str, names: &BTreeSet<String>) -> Option<String> {
    line.split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
        .map(last_segment)
        .find(|token| names.contains(*token))
        .map(str::to_string)
}

/// Строки уровня элемента, где число тоже означало бы меру: объявления
/// псевдонимов и макро-вызовы.
///
/// Оба найдены в ревью как обходы `field_lines`: псевдоним
/// `= Carrier<f64>` числа в позиции поля не содержит, а макрос подставляет поле
/// уже после того, как сканер прочёл строку. Проверка на уровне элемента —
/// нулевой отступ, поэтому `assert!` и `matches!` внутри блоков сюда не попадают.
fn item_level_type_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (number, raw) in source.lines().enumerate() {
        if raw.starts_with(char::is_whitespace) {
            continue;
        }
        let line = raw.trim();
        let is_alias = line.starts_with("type ") || line.starts_with("pub type ");
        let is_macro_item = line.ends_with(");") && line.contains('!') && !line.starts_with("//");
        if is_alias || is_macro_item {
            out.push((number + 1, line.to_string()));
        }
    }
    out
}

/// Гейт: числового поля нет ни в одном типе модуля.
#[test]
fn no_cleanliness_type_carries_a_numeric_field() {
    let sources = module_sources();
    let numeric = numeric_aliases(&sources);

    for (name, source) in &sources {
        let lines = field_lines(source)
            .into_iter()
            .chain(item_level_type_lines(source));
        for (number, line) in lines {
            if let Some(found) = mentions_any(&line, &numeric) {
                panic!(
                    "{name}:{number}: числовой тип `{found}` в позиции поля — \
                     `{line}`. Раздел 4 контракта запрещает меру грязности; \
                     дискриминанты enum числовым полем не являются и записи не \
                     образуют."
                );
            }
        }
    }
}

/// Гейт: сам разбор не вакуумен.
///
/// Каждая половина проверяется отдельно: что каталог обходится и находит файлы,
/// что поля распознаются, что числа ловятся во всех трёх формах и что имена
/// типов на подстроку не срабатывают.
#[test]
fn the_field_scanner_actually_reads_the_source() {
    let sources = module_sources();
    assert!(
        sources.len() >= 5,
        "каталог модуля обязан читаться целиком, найдено {}",
        sources.len()
    );
    assert!(
        sources.iter().any(|(name, _)| name == "report.rs"),
        "обход каталога обязан находить report.rs"
    );

    let report = &sources
        .iter()
        .find(|(name, _)| name == "report.rs")
        .expect("report.rs есть в модуле")
        .1;
    let fields = field_lines(report);
    assert!(
        fields.iter().any(|(_, line)| line.starts_with("mode:")),
        "сканер обязан находить поле `mode` в QualityReportV1"
    );

    let numeric = numeric_aliases(&sources);
    assert_eq!(
        mentions_any("score: f64,", &numeric).as_deref(),
        Some("f64")
    );
    assert_eq!(mentions_any("n: u8,", &numeric).as_deref(), Some("u8"));
    assert_eq!(mentions_any("mode: QualityModeV1,", &numeric), None);
    assert_eq!(mentions_any("confusable: Nou8Type,", &numeric), None);

    // Псевдоним путём — обход, найденный в ревью.
    assert_eq!(
        mentions_any("raw: core::primitive::u8,", &numeric).as_deref(),
        Some("u8"),
        "число, записанное путём, обязано ловиться"
    );

    let aliased = numeric_aliases(&[(
        "t.rs".to_string(),
        "type ScaleV1 = core::primitive::u8;\ntype WrapV1 = ScaleV1;\n".to_string(),
    )]);
    assert!(
        aliased.contains("ScaleV1"),
        "псевдоним путём обязан разворачиваться"
    );
    assert!(
        aliased.contains("WrapV1"),
        "цепочка псевдонимов обязана разворачиваться транзитивно"
    );

    assert!(declares_type("pub struct Score(f64);"));
    assert!(declares_type("pub(crate) struct Hidden {"));
    assert!(!declares_type("impl QualityReportV1 {"));
    let tuple = field_lines("pub struct CleanlinessScoreV1(pub f64);\n");
    assert_eq!(tuple.len(), 1, "кортежное объявление попадает в разбор");
}

/// Гейт: обе редакции границы совпадают дословно.
///
/// Утверждение «текст повторён дословно» дважды оказывалось ложным о самом
/// себе, поэтому теперь оно проверяется, а не обещается.
#[test]
fn both_boundary_texts_are_identical() {
    fn boundary(source: &str) -> String {
        let start = source
            .find("ГРАНИЦА ГЕЙТА")
            .expect("блок границы обязан присутствовать");
        let end = source[start..]
            .find("барьером быть не может")
            .expect("блок границы обязан замыкаться")
            + start;
        source[start..end]
            .lines()
            .map(|line| line.trim_start().trim_start_matches("//!").trim())
            .collect::<Vec<_>>()
            .join("\n")
    }

    let module =
        std::fs::read_to_string(src_dir().join("cleanliness.rs")).expect("шапка модуля читается");
    let gate = std::fs::read_to_string(
        src_dir()
            .parent()
            .expect("у src есть родитель")
            .join("tests/cleanliness_no_score_field.rs"),
    )
    .expect("шапка гейта читается");

    let left = boundary(&module);
    let right = boundary(&gate);
    assert!(!left.is_empty(), "блок границы не должен быть пустым");
    assert_eq!(
        left, right,
        "две редакции границы разошлись — ровно тот дефект, ради которого тест написан"
    );
}
