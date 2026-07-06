//! CLI харнесса психофизики. Подкоманды:
//!
//! - `generate` — построить манифест сессии (детерминированный из seed) и
//!   автономный HTML-раннер из паспорта labui.
//! - `analyze`  — подогнать логистику, посчитать bootstrap 95% CI PSE и вынести
//!   вердикт по критерию приёмки, читая сырые экспорты раннера (по файлу на
//!   наблюдателя).
//! - `selftest` — прогнать конвейер на синтетическом наблюдателе (известный PSE)
//!   и показать восстановление; та же логика, что замок честности в CI.
//! - `simulate` — записать сырые экспорты синтетических наблюдателей на диск,
//!   чтобы прогнать `analyze` вживую без набора людей (dry-run/power-анализ).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use psychophysics::analysis::{self, Session};
use psychophysics::logistic::Point;
use psychophysics::stimulus::{Acceptance, DesignParams, Manifest, build_session};
use psychophysics::synthetic::{self, Population};
use psychophysics::{color, html, json, passport};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    let code = match cmd {
        "generate" => cmd_generate(rest),
        "analyze" => cmd_analyze(rest),
        "selftest" => cmd_selftest(rest),
        "simulate" => cmd_simulate(rest),
        "-h" | "--help" | "help" | "" => {
            print_usage();
            Ok(0)
        }
        other => Err(format!("неизвестная подкоманда '{other}'.\n\n{USAGE}")),
    };

    match code {
        Ok(c) => ExitCode::from(c),
        Err(msg) => {
            eprintln!("ошибка: {msg}");
            ExitCode::from(1)
        }
    }
}

const USAGE: &str = "\
psychophysics — харнесс калибровки PAIR_CROSSOVER_Y (lab-colors)

ИСПОЛЬЗОВАНИЕ:
  psychophysics generate [--seed N] [--passport PATH] [--ink #HEX]
                         [--chroma-frac F] [--out-dir DIR]
  psychophysics analyze  <raw1.json> [raw2.json ...] [--resamples N] [--seed N]
  psychophysics selftest [--observers N] [--resamples N] [--seed N]
  psychophysics simulate [--observers N] [--pse P] [--slope S] [--pse-sd SD]
                         [--seed N] [--out-dir DIR]

generate  Строит session-<seed>.json (манифест) и session-<seed>.html (раннер).
          Один seed → идентичный манифест. Для N наблюдателей раздайте один
          html либо сгенерируйте разные seed для контрбаланса.
analyze   Каждый файл = сырой экспорт одного наблюдателя. Печатает вердикт JSON.
          Выход: 0 — принять, 3 — эскалация, 1 — ошибка.
selftest  Синтетический наблюдатель с истинным PSE=0.30; печатает восстановление.
          Выход: 0 — восстановлено в допуске, 4 — нет.
simulate  Пишет сырые экспорты синтетических наблюдателей (raw-*.json) в out-dir,
          чтобы прогнать `analyze raw-*.json` вживую без набора людей.";

fn print_usage() {
    println!("{USAGE}");
}

// ── парсинг флагов ───────────────────────────────────────────────────────────

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn parse_u64(args: &[String], name: &str, default: u64) -> Result<u64, String> {
    match flag_value(args, name) {
        Some(v) => v
            .parse::<u64>()
            .map_err(|e| format!("{name}: '{v}' не число ({e})")),
        None => Ok(default),
    }
}

fn parse_f64(args: &[String], name: &str, default: f64) -> Result<f64, String> {
    match flag_value(args, name) {
        Some(v) => v
            .parse::<f64>()
            .map_err(|e| format!("{name}: '{v}' не число ({e})")),
        None => Ok(default),
    }
}

/// Позиционные аргументы = всё, что не является флагом и не значением флага.
fn positionals(args: &[String], value_flags: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") {
            if value_flags.contains(&a.as_str()) {
                i += 2; // пропускаем флаг и его значение
            } else {
                i += 1;
            }
        } else {
            out.push(a.clone());
            i += 1;
        }
    }
    out
}

// ── generate ─────────────────────────────────────────────────────────────────

fn default_passport_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(passport::default_passport_relpath())
}

fn cmd_generate(args: &[String]) -> Result<u8, String> {
    let seed = parse_u64(args, "--seed", 1)?;
    let ink = flag_value(args, "--ink").unwrap_or_else(|| "#101012".to_string());
    let chroma_frac = parse_f64(args, "--chroma-frac", 0.9)?;
    let out_dir = flag_value(args, "--out-dir").unwrap_or_else(|| "psychophysics-out".to_string());

    let passport_path = flag_value(args, "--passport")
        .map(PathBuf::from)
        .unwrap_or_else(default_passport_path);
    let text = std::fs::read_to_string(&passport_path)
        .map_err(|e| format!("не прочитать паспорт {}: {e}", passport_path.display()))?;
    let families = passport::families_from_passport(&text)?;

    // Валидируем чернильный цвет заранее.
    color::hex_to_rgb(&ink).map_err(|e| format!("--ink: {e}"))?;

    let design = DesignParams {
        chroma_frac,
        ..DesignParams::default()
    };
    let manifest = build_session(&families, design, Acceptance::default(), &ink, seed);

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("не создать {out_dir}: {e}"))?;
    let manifest_path = format!("{out_dir}/session-{seed}.json");
    let html_path = format!("{out_dir}/session-{seed}.html");
    std::fs::write(&manifest_path, manifest.to_pretty_json())
        .map_err(|e| format!("не записать {manifest_path}: {e}"))?;
    std::fs::write(&html_path, html::render(&manifest))
        .map_err(|e| format!("не записать {html_path}: {e}"))?;

    println!("сгенерировано (seed={seed}):");
    println!("  манифест: {manifest_path}");
    println!("  раннер:   {html_path}  ← откройте в браузере");
    println!(
        "  {} проб · {} семей · Y ∈ [{:.2}, {:.2}] шаг {:.2} · чернила {ink}",
        manifest.trials.len(),
        manifest.families.len(),
        manifest.design.y_min,
        manifest
            .y_grid
            .last()
            .copied()
            .unwrap_or(manifest.design.y_max),
        manifest.design.y_step,
    );
    Ok(0)
}

// ── analyze ──────────────────────────────────────────────────────────────────

fn session_from_raw(text: &str, fallback: &str) -> Result<Session, String> {
    let v = json::parse(text)?;
    let observer = v
        .get("observer")
        .and_then(json::Value::as_str)
        .unwrap_or(fallback)
        .to_string();
    let responses = v
        .get("responses")
        .and_then(json::Value::as_array)
        .ok_or("в экспорте нет массива 'responses'")?;
    let mut points = Vec::with_capacity(responses.len());
    for (i, r) in responses.iter().enumerate() {
        let y = r
            .get("measured_y")
            .and_then(json::Value::as_f64)
            .ok_or_else(|| format!("responses[{i}]: нет 'measured_y'"))?;
        let chose_white = r
            .get("chose_white")
            .and_then(json::Value::as_bool)
            .ok_or_else(|| format!("responses[{i}]: нет 'chose_white'"))?;
        points.push(Point { y, chose_white });
    }
    Ok(Session { observer, points })
}

fn cmd_analyze(args: &[String]) -> Result<u8, String> {
    let resamples = parse_u64(args, "--resamples", 2000)? as usize;
    let seed = parse_u64(args, "--seed", 12345)?;
    let paths = positionals(args, &["--resamples", "--seed"]);
    if paths.is_empty() {
        return Err(format!("нужен хотя бы один сырой JSON.\n\n{USAGE}"));
    }
    if resamples < 2000 {
        eprintln!("предупреждение: протокол требует ≥ 2000 ресэмплов (задано {resamples}).");
    }

    let mut sessions = Vec::new();
    for p in &paths {
        let text = std::fs::read_to_string(p).map_err(|e| format!("не прочитать {p}: {e}"))?;
        let s = session_from_raw(&text, p)?;
        sessions.push(s);
    }
    let n_obs = sessions.len();
    let n_resp: usize = sessions.iter().map(|s| s.points.len()).sum();

    let boot = analysis::bootstrap_pse(&sessions, resamples, seed)
        .ok_or("подгонка не удалась (недостаточно вариации в данных?)")?;
    let verdict = analysis::evaluate(&boot, &Acceptance::default());

    println!("{}", verdict.to_json(&boot).to_pretty());
    println!("\n— {n_obs} наблюдателей, {n_resp} ответов —");
    println!(
        "PSE={:.4}  95% CI=[{:.4}, {:.4}]  ширина={:.4}",
        boot.pse, boot.ci_lo, boot.ci_hi, boot.ci_width
    );
    println!("{}: {}", verdict.decision.as_str(), verdict.reason);

    Ok(match verdict.decision {
        analysis::Decision::Accept => 0,
        analysis::Decision::Escalate => 3,
    })
}

// ── selftest ─────────────────────────────────────────────────────────────────

fn cmd_selftest(args: &[String]) -> Result<u8, String> {
    let observers = parse_u64(args, "--observers", 18)? as usize;
    let resamples = parse_u64(args, "--resamples", 2000)? as usize;
    let seed = parse_u64(args, "--seed", 20_250_706)?;

    // Манифест из паспорта (или фолбэк), затем синтетическая популяция PSE=0.30.
    let families = psychophysics::stimulus::families_or_fallback();
    let manifest = build_session(
        &families,
        DesignParams::default(),
        Acceptance::default(),
        "#101012",
        seed,
    );
    let pop = Population::calibration_default();
    let sessions = synthetic::simulate_population(&manifest, pop, observers, seed ^ 0xABCD);

    let boot = analysis::bootstrap_pse(&sessions, resamples, seed.wrapping_add(1))
        .ok_or("bootstrap не сошёлся на синтетике")?;
    let verdict = analysis::evaluate(&boot, &Acceptance::default());
    let err = (boot.pse - pop.pse).abs();

    println!(
        "SELFTEST · истинный PSE={:.3}, наблюдателей {observers}, ресэмплов {resamples}",
        pop.pse
    );
    println!("восстановленный PSE={:.4}  |ошибка|={err:.4}", boot.pse);
    println!(
        "95% CI=[{:.4}, {:.4}]  ширина={:.4}",
        boot.ci_lo, boot.ci_hi, boot.ci_width
    );
    let covers = boot.ci_lo <= pop.pse && pop.pse <= boot.ci_hi;
    println!(
        "истинный PSE внутри CI: {}",
        if covers { "да" } else { "нет" }
    );
    println!(
        "вердикт: {} — {}",
        verdict.decision.as_str(),
        verdict.reason
    );

    // Замок: восстановление в допуске 0.02 и истинный PSE покрыт CI.
    if err < 0.02 && covers {
        println!("РЕЗУЛЬТАТ: конвейер восстановил истину ✓");
        Ok(0)
    } else {
        println!("РЕЗУЛЬТАТ: восстановление вне допуска ✗");
        Ok(4)
    }
}

// ── simulate ─────────────────────────────────────────────────────────────────

/// Сырой экспорт одного синтетического наблюдателя в формате раннера.
///
/// Точки наблюдателя выровнены 1:1 с `manifest.trials` (см. `simulate_observer`),
/// поэтому восстанавливаем полную запись пробы: сторону ответа выводим из
/// `chose_white` и `white_side`.
fn synthetic_raw_export(manifest: &Manifest, observer: &str, points: &[Point]) -> json::Value {
    let responses: Vec<json::Value> = manifest
        .trials
        .iter()
        .zip(points)
        .map(|(t, p)| {
            let response_side = if p.chose_white {
                t.white_side.as_str()
            } else if t.white_side.as_str() == "left" {
                "right"
            } else {
                "left"
            };
            json::obj(vec![
                ("id", json::Value::Number(t.id as f64)),
                ("family", json::Value::String(t.family.clone())),
                ("target_y", json::Value::Number(t.target_y)),
                ("measured_y", json::Value::Number(t.measured_y)),
                ("swatch_hex", json::Value::String(t.swatch_hex.clone())),
                (
                    "white_side",
                    json::Value::String(t.white_side.as_str().to_string()),
                ),
                (
                    "response_side",
                    json::Value::String(response_side.to_string()),
                ),
                ("chose_white", json::Value::Bool(p.chose_white)),
                ("rt_ms", json::Value::Number(0.0)),
            ])
        })
        .collect();

    json::obj(vec![
        ("harness", json::Value::String(manifest.harness.clone())),
        ("target", json::Value::String(manifest.target.clone())),
        ("version", json::Value::Number(f64::from(manifest.version))),
        ("seed", json::Value::Number(manifest.seed as f64)),
        ("observer", json::Value::String(observer.to_string())),
        ("started_utc", json::Value::String("synthetic".to_string())),
        ("finished_utc", json::Value::String("synthetic".to_string())),
        (
            "conditions",
            json::obj(vec![
                ("synthetic", json::Value::Bool(true)),
                ("normal_cv", json::Value::Bool(true)),
                ("calibrated", json::Value::Bool(true)),
                ("d65", json::Value::Bool(true)),
                ("distance_ok", json::Value::Bool(true)),
                ("fullscreen_100", json::Value::Bool(true)),
            ]),
        ),
        (
            "manifest_n_trials",
            json::Value::Number(manifest.trials.len() as f64),
        ),
        ("responses", json::Value::Array(responses)),
    ])
}

fn cmd_simulate(args: &[String]) -> Result<u8, String> {
    let observers = parse_u64(args, "--observers", 18)? as usize;
    let pse = parse_f64(args, "--pse", 0.30)?;
    let slope = parse_f64(args, "--slope", -45.0)?;
    let pse_sd = parse_f64(args, "--pse-sd", 0.02)?;
    let seed = parse_u64(args, "--seed", 20_250_706)?;
    let out_dir = flag_value(args, "--out-dir").unwrap_or_else(|| "psychophysics-out".to_string());

    let manifest = build_session(
        &psychophysics::stimulus::families_or_fallback(),
        DesignParams::default(),
        Acceptance::default(),
        "#101012",
        seed,
    );
    let pop = Population { pse, slope, pse_sd };
    let sessions = synthetic::simulate_population(&manifest, pop, observers, seed ^ 0xABCD);

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("не создать {out_dir}: {e}"))?;
    for s in &sessions {
        let export = synthetic_raw_export(&manifest, &s.observer, &s.points);
        let path = format!("{out_dir}/raw-{}.json", s.observer);
        std::fs::write(&path, export.to_pretty())
            .map_err(|e| format!("не записать {path}: {e}"))?;
    }

    println!(
        "записано {observers} сырых экспортов в {out_dir}/raw-*.json (истинный PSE={pse:.3}).",
    );
    println!("проверьте пайплайн: psychophysics analyze {out_dir}/raw-*.json");
    Ok(0)
}
