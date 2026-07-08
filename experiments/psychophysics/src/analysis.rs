//! Аналитический пайплайн: точечная оценка PSE, bootstrap 95% CI и вердикт по
//! критерию приёмки мишени #1.
//!
//! Bootstrap — КЛАСТЕРНЫЙ по наблюдателям (ресэмпл наблюдателей с возвращением),
//! потому что CI обязан отражать межнаблюдательную дисперсию: N≥15 человек — это
//! выборка из популяции, и обобщать надо на популяцию, а не на конкретные тяги.
//! При единственном наблюдателе метод корректно вырождается в trial-level ресэмпл.
//! Точечная оценка — подгонка на всём пуле (не среднее bootstrap, чтобы не
//! смещать). CI — перцентильный `[2.5, 97.5]`.

use crate::json::{Value, obj};
use crate::logistic::{self, Point};
use crate::rng::SplitMix64;
use crate::stimulus::{Acceptance, round6};

/// Минимальная доля сошедшихся bootstrap-ресэмплов для вынесения вердикта.
/// Вырожденные подгонки выбрасываются из CI; если уцелели немногие,
/// перцентильный CI по выжившим может быть обманчиво узким — тогда
/// вердикт не выносится, только эскалация.
const MIN_SUCCESS_FRAC: f64 = 0.95;

/// Сессия одного наблюдателя.
#[derive(Debug, Clone)]
pub struct Session {
    pub observer: String,
    pub points: Vec<Point>,
}

/// Итог bootstrap.
#[derive(Debug, Clone, Copy)]
pub struct BootResult {
    /// Точечная оценка PSE (подгонка на всём пуле).
    pub pse: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
    pub ci_width: f64,
    /// Сколько ресэмплов дали валидную подгонку.
    pub n_success: usize,
    pub n_resamples: usize,
}

/// Решение по приёмке.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Escalate,
}

impl Decision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Accept => "ACCEPT",
            Decision::Escalate => "ESCALATE",
        }
    }
}

/// Вердикт с обоснованием.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub decision: Decision,
    pub reason: String,
    pub pse: f64,
    pub ci_lo: f64,
    pub ci_hi: f64,
    pub ci_width: f64,
    pub ci_ok: bool,
    pub pse_in_interval: bool,
    /// Достаточна ли доля сошедшихся ресэмплов (`≥ MIN_SUCCESS_FRAC`).
    pub success_ok: bool,
}

/// Собрать все точки в один пул.
#[must_use]
pub fn pooled_points(sessions: &[Session]) -> Vec<Point> {
    sessions
        .iter()
        .flat_map(|s| s.points.iter().copied())
        .collect()
}

/// Точечная оценка PSE на всём пуле.
#[must_use]
pub fn point_estimate(sessions: &[Session]) -> Option<f64> {
    logistic::fit_pse(&pooled_points(sessions))
}

/// Перцентиль (тип 7, линейная интерполяция) на отсортированном срезе.
///
/// `q ∈ [0,1]`. Пустой срез → `None`.
#[must_use]
pub fn percentile_sorted(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    if sorted.len() == 1 {
        return Some(sorted[0]);
    }
    let h = (sorted.len() as f64 - 1.0) * q.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    let frac = h - lo as f64;
    Some(sorted[lo] + frac * (sorted[hi] - sorted[lo]))
}

/// Кластерный bootstrap PSE. `n_resamples ≥ 1`; протокол требует ≥ 2000.
///
/// Каждый ресэмпл: тянем `len(sessions)` наблюдателей с возвращением, пулим их
/// точки, подгоняем PSE. Провальные подгонки (вырожденный ресэмпл) пропускаются;
/// CI считается по успешным. `None`, если пул пуст или ни один ресэмпл не сошёлся.
#[must_use]
pub fn bootstrap_pse(sessions: &[Session], n_resamples: usize, seed: u64) -> Option<BootResult> {
    if sessions.is_empty() {
        return None;
    }
    let point = point_estimate(sessions)?;
    let mut rng = SplitMix64::new(seed);
    let n_obs = sessions.len();
    let mut pses: Vec<f64> = Vec::with_capacity(n_resamples);

    for _ in 0..n_resamples {
        // Кластерный ресэмпл наблюдателей (или единственного — тогда trial-level
        // ресэмпл его точек, чтобы CI не был вырожденно нулевым).
        let sample_points: Vec<Point> = if n_obs == 1 {
            let src = &sessions[0].points;
            if src.is_empty() {
                continue;
            }
            (0..src.len())
                .map(|_| src[rng.below(src.len() as u64) as usize])
                .collect()
        } else {
            let mut acc = Vec::new();
            for _ in 0..n_obs {
                let idx = rng.below(n_obs as u64) as usize;
                acc.extend(sessions[idx].points.iter().copied());
            }
            acc
        };
        if let Some(v) = logistic::fit_pse(&sample_points) {
            pses.push(v);
        }
    }

    if pses.is_empty() {
        return None;
    }
    pses.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let ci_lo = percentile_sorted(&pses, 0.025)?;
    let ci_hi = percentile_sorted(&pses, 0.975)?;

    Some(BootResult {
        pse: round6(point),
        ci_lo: round6(ci_lo),
        ci_hi: round6(ci_hi),
        ci_width: round6(ci_hi - ci_lo),
        n_success: pses.len(),
        n_resamples,
    })
}

/// Оценить итог bootstrap против критерия приёмки.
///
/// Принять ⟺ сошлось `≥ MIN_SUCCESS_FRAC` ресэмплов И ширина CI `< ci_width_max`
/// И PSE в `(pse_lo, pse_hi)`. Иначе — эскалация владельцу с указанием, какое
/// условие нарушено.
#[must_use]
pub fn evaluate(boot: &BootResult, acc: &Acceptance) -> Verdict {
    let ci_ok = boot.ci_width < acc.ci_width_max;
    let pse_in_interval = boot.pse > acc.pse_lo && boot.pse < acc.pse_hi;
    let success_ok =
        boot.n_resamples > 0 && boot.n_success as f64 >= MIN_SUCCESS_FRAC * boot.n_resamples as f64;

    let (decision, reason) = if !success_ok {
        (
            Decision::Escalate,
            format!(
                "Сошлись лишь {}/{} bootstrap-ресэмплов (< {:.0}%): CI по уцелевшим ненадёжен. Эскалация.",
                boot.n_success,
                boot.n_resamples,
                MIN_SUCCESS_FRAC * 100.0
            ),
        )
    } else if ci_ok && pse_in_interval {
        (
            Decision::Accept,
            format!(
                "CI-ширина {:.4} < {:.4} И PSE {:.4} ∈ ({:.3}, {:.3}) → принять значение {:.3}.",
                boot.ci_width, acc.ci_width_max, boot.pse, acc.pse_lo, acc.pse_hi, acc.target_value
            ),
        )
    } else if !ci_ok {
        (
            Decision::Escalate,
            format!(
                "CI-ширина {:.4} ≥ {:.4}: точность недостаточна (нужно больше наблюдателей/проб). Эскалация.",
                boot.ci_width, acc.ci_width_max
            ),
        )
    } else {
        (
            Decision::Escalate,
            format!(
                "PSE {:.4} вне текущего интервала ({:.3}, {:.3}): значение {:.3} под вопросом. Эскалация владельцу.",
                boot.pse, acc.pse_lo, acc.pse_hi, acc.target_value
            ),
        )
    };

    Verdict {
        decision,
        reason,
        pse: boot.pse,
        ci_lo: boot.ci_lo,
        ci_hi: boot.ci_hi,
        ci_width: boot.ci_width,
        ci_ok,
        pse_in_interval,
        success_ok,
    }
}

impl Verdict {
    /// Сериализовать вердикт для вывода `analyze`.
    #[must_use]
    pub fn to_json(&self, boot: &BootResult) -> Value {
        obj(vec![
            (
                "decision",
                Value::String(self.decision.as_str().to_string()),
            ),
            ("reason", Value::String(self.reason.clone())),
            ("pse", Value::Number(self.pse)),
            ("ci95_lo", Value::Number(self.ci_lo)),
            ("ci95_hi", Value::Number(self.ci_hi)),
            ("ci95_width", Value::Number(self.ci_width)),
            ("ci_ok", Value::Bool(self.ci_ok)),
            ("pse_in_interval", Value::Bool(self.pse_in_interval)),
            ("bootstrap_success_ok", Value::Bool(self.success_ok)),
            (
                "bootstrap_resamples",
                Value::Number(boot.n_resamples as f64),
            ),
            ("bootstrap_success", Value::Number(boot.n_success as f64)),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_endpoints_and_mid() {
        let v = vec![0.0, 1.0, 2.0, 3.0, 4.0];
        assert!((percentile_sorted(&v, 0.0).unwrap() - 0.0).abs() < 1e-12);
        assert!((percentile_sorted(&v, 1.0).unwrap() - 4.0).abs() < 1e-12);
        assert!((percentile_sorted(&v, 0.5).unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn percentile_empty_none() {
        assert!(percentile_sorted(&[], 0.5).is_none());
    }

    #[test]
    fn verdict_accept_path() {
        let boot = BootResult {
            pse: 0.30,
            ci_lo: 0.29,
            ci_hi: 0.31,
            ci_width: 0.02,
            n_success: 2000,
            n_resamples: 2000,
        };
        let v = evaluate(&boot, &Acceptance::default());
        assert_eq!(v.decision, Decision::Accept);
    }

    #[test]
    fn verdict_escalates_on_wide_ci() {
        let boot = BootResult {
            pse: 0.30,
            ci_lo: 0.25,
            ci_hi: 0.35,
            ci_width: 0.10,
            n_success: 2000,
            n_resamples: 2000,
        };
        let v = evaluate(&boot, &Acceptance::default());
        assert_eq!(v.decision, Decision::Escalate);
        assert!(!v.ci_ok);
    }

    #[test]
    fn verdict_escalates_on_low_bootstrap_success() {
        // Узкий CI по горстке уцелевших ресэмплов не должен давать Accept.
        let boot = BootResult {
            pse: 0.30,
            ci_lo: 0.29,
            ci_hi: 0.31,
            ci_width: 0.02,
            n_success: 1200,
            n_resamples: 2000,
        };
        let v = evaluate(&boot, &Acceptance::default());
        assert_eq!(v.decision, Decision::Escalate);
        assert!(!v.success_ok);
    }

    #[test]
    fn verdict_escalates_on_pse_outside_interval() {
        let boot = BootResult {
            pse: 0.20,
            ci_lo: 0.195,
            ci_hi: 0.205,
            ci_width: 0.01,
            n_success: 2000,
            n_resamples: 2000,
        };
        let v = evaluate(&boot, &Acceptance::default());
        assert_eq!(v.decision, Decision::Escalate);
        assert!(!v.pse_in_interval);
    }
}
