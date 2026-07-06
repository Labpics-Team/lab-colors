//! Генератор стимулов: детерминированный манифест сессии для мишени #1
//! (`PAIR_CROSSOVER_Y`).
//!
//! Из паспорта labui берутся фирменные семьи; для каждой строится ряд свотчей с
//! люминанс Y по сетке `[0.18, 0.45]` шаг ~0.02 (`color::swatch_at_luminance`).
//! Каждый свотч предъявляется как 2AFC: тот же цвет с БЕЛЫМ и с ЧЕРНИЛЬНЫМ
//! лейблом; наблюдатель выбирает, где лейбл читается. Порядок предъявлений и
//! сторона белого лейбла рандомизируются из seed (Fisher-Yates), стороны
//! сбалансированы 50/50. Один seed → байт-идентичный манифест.

use crate::color;
use crate::json::{Value, obj};
use crate::passport::Family;
use crate::rng::SplitMix64;

/// Критерий приёмки мишени (из `docs/empirical-residue.md` §1).
#[derive(Debug, Clone, Copy)]
pub struct Acceptance {
    /// Максимальная допустимая ширина 95% CI (в единицах Y).
    pub ci_width_max: f64,
    /// Нижняя граница текущего интервала значения.
    pub pse_lo: f64,
    /// Верхняя граница текущего интервала значения.
    pub pse_hi: f64,
    /// Текущее задекларированное значение константы.
    pub target_value: f64,
}

impl Default for Acceptance {
    fn default() -> Self {
        // Порог CI и интервал (0.246, 0.423) — из протокола §1; текущее значение 0.30.
        Self {
            ci_width_max: 0.03,
            pse_lo: 0.246,
            pse_hi: 0.423,
            target_value: 0.30,
        }
    }
}

/// Параметры дизайна стимулов.
#[derive(Debug, Clone, Copy)]
pub struct DesignParams {
    /// Нижняя граница люминанс сетки.
    pub y_min: f64,
    /// Верхняя граница люминанс сетки.
    pub y_max: f64,
    /// Шаг сетки люминанс.
    pub y_step: f64,
    /// Доля предельной хромы гаммы для свотчей (0..1].
    pub chroma_frac: f64,
}

impl Default for DesignParams {
    fn default() -> Self {
        Self {
            y_min: 0.18,
            y_max: 0.45,
            y_step: 0.02,
            chroma_frac: 0.9,
        }
    }
}

/// Один 2AFC-пробник.
#[derive(Debug, Clone)]
pub struct Trial {
    /// Стабильный id стимула (`family_index*grid + y_index`) — не зависит от порядка.
    pub id: usize,
    /// Семья оттенка.
    pub family: String,
    /// Номинальная люминанс сетки.
    pub target_y: f64,
    /// Hex свотча.
    pub swatch_hex: String,
    /// Фактическая (показанная) люминанс свотча после квантования в 8 бит.
    pub measured_y: f64,
    /// Контраст WCAG свотча с белым лейблом.
    pub contrast_white: f64,
    /// Контраст WCAG свотча с чернильным лейблом.
    pub contrast_ink: f64,
    /// Сторона белого лейбла: `"left"` или `"right"` (другая — чернильный).
    pub white_side: Side,
}

/// Сторона предъявления в 2AFC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// Полный манифест сессии.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub harness: String,
    pub target: String,
    pub version: u32,
    pub seed: u64,
    pub white_hex: String,
    pub ink_hex: String,
    pub design: DesignParams,
    pub acceptance: Acceptance,
    pub y_grid: Vec<f64>,
    pub families: Vec<String>,
    /// Пробники в РАНДОМИЗИРОВАННОМ порядке предъявления.
    pub trials: Vec<Trial>,
}

/// Сетка люминанс `[y_min, y_max]` шагом `y_step` (включая узлы ≤ y_max).
#[must_use]
pub fn luminance_grid(p: &DesignParams) -> Vec<f64> {
    let mut grid = Vec::new();
    let mut y = p.y_min;
    // eps защищает от промаха последнего узла из-за плавучей арифметики.
    while y <= p.y_max + 1e-9 {
        grid.push(round6(y));
        y += p.y_step;
    }
    grid
}

/// Округление до 6 знаков — чистый детерминированный вывод.
#[must_use]
pub fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Построить манифест сессии из семей, параметров и seed.
///
/// Чернильный лейбл `ink_hex` — тёмный «чернильный» цвет (по умолчанию `#101012`,
/// нейтральное ребро паспорта); белый — `#FFFFFF`.
///
/// # Errors
/// `Err`, если `ink_hex` не парсится: для калибровочного инструмента тихая
/// подмена цвета лейбла означала бы сессию с НЕВЕРНЫМ стимулом, незаметную ни в
/// манифесте, ни в отчёте, — поэтому явный отказ вместо `unwrap_or`-дефолта.
pub fn build_session(
    families: &[Family],
    design: DesignParams,
    acceptance: Acceptance,
    ink_hex: &str,
    seed: u64,
) -> Result<Manifest, String> {
    let grid = luminance_grid(&design);
    let ink_rgb =
        color::hex_to_rgb(ink_hex).map_err(|e| format!("некорректный ink_hex '{ink_hex}': {e}"))?;
    let white_rgb = [255u8, 255, 255];

    // 1) Канонический набор стимулов (family-major, затем по Y). id стабилен.
    let mut trials: Vec<Trial> = Vec::with_capacity(families.len() * grid.len());
    for (fi, fam) in families.iter().enumerate() {
        for (yi, &y) in grid.iter().enumerate() {
            let (swatch_hex, measured_y) =
                color::swatch_at_luminance(fam.anchor_rgb, y, design.chroma_frac);
            let swatch_rgb = color::hex_to_rgb(&swatch_hex).unwrap_or([0, 0, 0]);
            trials.push(Trial {
                id: fi * grid.len() + yi,
                family: fam.key.clone(),
                target_y: y,
                swatch_hex,
                measured_y: round6(measured_y),
                contrast_white: round6(color::wcag_contrast(swatch_rgb, white_rgb)),
                contrast_ink: round6(color::wcag_contrast(swatch_rgb, ink_rgb)),
                white_side: Side::Left, // назначим сбалансированно ниже
            });
        }
    }

    let mut rng = SplitMix64::new(seed);

    // 2) Сбалансированные стороны 50/50, затем перемешиваем и раздаём.
    let n = trials.len();
    let mut sides: Vec<Side> = (0..n)
        .map(|i| if i < n / 2 { Side::Left } else { Side::Right })
        .collect();
    rng.shuffle(&mut sides);
    for (t, s) in trials.iter_mut().zip(sides) {
        t.white_side = s;
    }

    // 3) Рандомизация порядка предъявления.
    rng.shuffle(&mut trials);

    Ok(Manifest {
        harness: "labcolors-psychophysics".to_string(),
        target: "PAIR_CROSSOVER_Y".to_string(),
        version: 1,
        seed,
        white_hex: "#FFFFFF".to_string(),
        ink_hex: ink_hex.to_string(),
        design,
        acceptance,
        y_grid: grid,
        families: families.iter().map(|f| f.key.clone()).collect(),
        trials,
    })
}

impl Manifest {
    /// Сериализовать в `json::Value` (стабильный порядок ключей).
    #[must_use]
    pub fn to_json(&self) -> Value {
        let trials: Vec<Value> = self
            .trials
            .iter()
            .map(|t| {
                obj(vec![
                    ("id", Value::Number(t.id as f64)),
                    ("family", Value::String(t.family.clone())),
                    ("target_y", Value::Number(t.target_y)),
                    ("swatch_hex", Value::String(t.swatch_hex.clone())),
                    ("measured_y", Value::Number(t.measured_y)),
                    ("contrast_white", Value::Number(t.contrast_white)),
                    ("contrast_ink", Value::Number(t.contrast_ink)),
                    (
                        "white_side",
                        Value::String(t.white_side.as_str().to_string()),
                    ),
                ])
            })
            .collect();

        obj(vec![
            ("harness", Value::String(self.harness.clone())),
            ("target", Value::String(self.target.clone())),
            ("version", Value::Number(f64::from(self.version))),
            // seed как СТРОКА: u64 > 2^53 теряет точность в f64, и тогда манифест
            // невоспроизводим по собственной записи. Строка держит зерно точно;
            // никто не читает seed обратно как число (analyze берёт только
            // observer/responses).
            ("seed", Value::String(self.seed.to_string())),
            ("white_hex", Value::String(self.white_hex.clone())),
            ("ink_hex", Value::String(self.ink_hex.clone())),
            (
                "design",
                obj(vec![
                    ("y_min", Value::Number(self.design.y_min)),
                    ("y_max", Value::Number(self.design.y_max)),
                    ("y_step", Value::Number(self.design.y_step)),
                    ("chroma_frac", Value::Number(self.design.chroma_frac)),
                ]),
            ),
            (
                "acceptance",
                obj(vec![
                    ("ci_width_max", Value::Number(self.acceptance.ci_width_max)),
                    ("pse_lo", Value::Number(self.acceptance.pse_lo)),
                    ("pse_hi", Value::Number(self.acceptance.pse_hi)),
                    ("target_value", Value::Number(self.acceptance.target_value)),
                ]),
            ),
            (
                "y_grid",
                Value::Array(self.y_grid.iter().map(|&y| Value::Number(y)).collect()),
            ),
            (
                "families",
                Value::Array(
                    self.families
                        .iter()
                        .map(|f| Value::String(f.clone()))
                        .collect(),
                ),
            ),
            ("n_trials", Value::Number(self.trials.len() as f64)),
            ("trials", Value::Array(trials)),
        ])
    }

    /// Компактный JSON для встраивания в раннер.
    #[must_use]
    pub fn to_json_string(&self) -> String {
        self.to_json().to_compact()
    }

    /// Читаемый JSON манифеста.
    #[must_use]
    pub fn to_pretty_json(&self) -> String {
        self.to_json().to_pretty()
    }
}

/// Хелпер для тестов и e2e: реальные семьи паспорта либо, если файла нет,
/// компактный синтетический набор из четырёх семей.
#[must_use]
pub fn families_or_fallback() -> Vec<Family> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
    let path = format!("{root}{}", crate::passport::default_passport_relpath());
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(f) = crate::passport::families_from_passport(&text)
    {
        return f;
    }
    ["#FF3B30", "#FFA100", "#34C759", "#007AFF"]
        .iter()
        .enumerate()
        .map(|(i, &hex)| Family {
            key: format!("fam{i}"),
            anchor_hex: hex.to_string(),
            anchor_rgb: color::hex_to_rgb(hex).unwrap(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_families() -> Vec<Family> {
        ["#FF3B30", "#007AFF", "#34C759"]
            .iter()
            .map(|&hex| Family {
                key: hex.to_string(),
                anchor_hex: hex.to_string(),
                anchor_rgb: color::hex_to_rgb(hex).unwrap(),
            })
            .collect()
    }

    #[test]
    fn grid_spans_range() {
        let g = luminance_grid(&DesignParams::default());
        assert_eq!(g[0], 0.18);
        assert!(*g.last().unwrap() <= 0.45 + 1e-9);
        assert!(*g.last().unwrap() >= 0.43); // близко к верху
        // 0.30 обязано быть узлом (кандидатное значение на сетке).
        assert!(g.iter().any(|&y| (y - 0.30).abs() < 1e-9));
    }

    #[test]
    fn same_seed_identical_manifest() {
        let f = demo_families();
        let a = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            777,
        )
        .unwrap();
        let b = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            777,
        )
        .unwrap();
        assert_eq!(a.to_json_string(), b.to_json_string());
    }

    #[test]
    fn different_seed_different_order() {
        let f = demo_families();
        let a = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            1,
        )
        .unwrap();
        let b = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            2,
        )
        .unwrap();
        let ord_a: Vec<usize> = a.trials.iter().map(|t| t.id).collect();
        let ord_b: Vec<usize> = b.trials.iter().map(|t| t.id).collect();
        assert_ne!(ord_a, ord_b, "разные seed → разный порядок");
    }

    #[test]
    fn trial_count_and_stable_ids() {
        let f = demo_families();
        let grid = luminance_grid(&DesignParams::default());
        let m = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            5,
        )
        .unwrap();
        assert_eq!(m.trials.len(), f.len() * grid.len());
        // id — перестановка 0..n.
        let mut ids: Vec<usize> = m.trials.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (0..f.len() * grid.len()).collect::<Vec<_>>());
    }

    #[test]
    fn sides_balanced() {
        let f = demo_families();
        let m = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            42,
        )
        .unwrap();
        let left = m
            .trials
            .iter()
            .filter(|t| t.white_side == Side::Left)
            .count();
        let n = m.trials.len();
        // Ровно floor(n/2) слева по построению.
        assert_eq!(left, n / 2);
    }

    #[test]
    fn manifest_json_reparses() {
        let f = demo_families();
        let m = build_session(
            &f,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            9,
        )
        .unwrap();
        let parsed = crate::json::parse(&m.to_pretty_json()).expect("манифест — валидный JSON");
        assert_eq!(
            parsed.get("target").unwrap().as_str().unwrap(),
            "PAIR_CROSSOVER_Y"
        );
        assert_eq!(
            parsed.get("trials").unwrap().as_array().unwrap().len(),
            m.trials.len()
        );
    }
}
