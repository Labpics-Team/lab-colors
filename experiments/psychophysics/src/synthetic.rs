//! Синтетический наблюдатель — замок честности пайплайна.
//!
//! Генерирует отклики известной логистикой `P(белый) = σ(b·(Y − PSE))` (наклон
//! `b<0`) с межнаблюдательным разбросом PSE. Если пайплайн (подгонка +
//! bootstrap) НЕ восстанавливает заложенный популяционный PSE в допуске — тест
//! падает: это ловит регресс, при котором анализ «подтверждает» что угодно
//! (assertion-free театр). Замок гоняется в CI через `cargo test --workspace`.

use crate::analysis::Session;
use crate::logistic::{Point, sigmoid};
use crate::rng::SplitMix64;
use crate::stimulus::Manifest;

/// Параметры синтетической популяции.
#[derive(Debug, Clone, Copy)]
pub struct Population {
    /// Истинный популяционный PSE (кроссовер).
    pub pse: f64,
    /// Логистический наклон (крутизна психометрики), должен быть отрицательным.
    pub slope: f64,
    /// СКО межнаблюдательного разброса PSE (0 → идентичные наблюдатели).
    pub pse_sd: f64,
}

impl Population {
    /// Разумный дефолт: истинный PSE = 0.30, крутой наклон, умеренный разброс.
    #[must_use]
    pub fn calibration_default() -> Self {
        Self {
            pse: 0.30,
            slope: -45.0,
            pse_sd: 0.02,
        }
    }
}

/// Стандартная нормаль через Бокса-Мюллера.
fn gaussian(rng: &mut SplitMix64) -> f64 {
    let u1 = rng.next_f64().max(1e-12);
    let u2 = rng.next_f64();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// Симулировать отклики ОДНОГО наблюдателя с заданным личным PSE и наклоном по
/// люминанс проб манифеста.
#[must_use]
pub fn simulate_observer(
    manifest: &Manifest,
    observer_pse: f64,
    slope: f64,
    seed: u64,
) -> Vec<Point> {
    let mut rng = SplitMix64::new(seed);
    manifest
        .trials
        .iter()
        .map(|t| {
            let p = sigmoid(slope * (t.measured_y - observer_pse));
            Point {
                y: t.measured_y,
                chose_white: rng.next_f64() < p,
            }
        })
        .collect()
}

/// Симулировать популяцию из `n_obs` наблюдателей; каждый получает личный
/// `PSE ~ N(pop.pse, pop.pse_sd)` и общий наклон.
#[must_use]
pub fn simulate_population(
    manifest: &Manifest,
    pop: Population,
    n_obs: usize,
    seed: u64,
) -> Vec<Session> {
    let mut rng = SplitMix64::new(seed);
    let mut sessions = Vec::with_capacity(n_obs);
    for i in 0..n_obs {
        let personal_pse = pop.pse + pop.pse_sd * gaussian(&mut rng);
        // Отдельный поток на отклики наблюдателя — детерминированный от seed+индекс.
        let obs_seed = rng.next_u64() ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let points = simulate_observer(manifest, personal_pse, pop.slope, obs_seed);
        sessions.push(Session {
            observer: format!("synthetic-{i:02}"),
            points,
        });
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passport::Family;
    use crate::stimulus::{Acceptance, DesignParams, build_session};
    use crate::{analysis, color};

    fn demo_manifest(seed: u64) -> Manifest {
        let families: Vec<Family> = ["#FF3B30", "#FFA100", "#34C759", "#007AFF", "#BF5AF2"]
            .iter()
            .map(|&hex| Family {
                key: hex.to_string(),
                anchor_hex: hex.to_string(),
                anchor_rgb: color::hex_to_rgb(hex).unwrap(),
            })
            .collect();
        build_session(
            &families,
            DesignParams::default(),
            Acceptance::default(),
            "#101012",
            seed,
        )
    }

    #[test]
    fn pooled_point_estimate_recovers_true_pse() {
        // Замок честности (быстрая версия без bootstrap): пул из 20 наблюдателей
        // восстанавливает истинный PSE=0.30 в тесном допуске.
        let manifest = demo_manifest(101);
        let pop = Population::calibration_default();
        let sessions = simulate_population(&manifest, pop, 20, 20250706);
        let pse = analysis::point_estimate(&sessions).expect("оценка PSE");
        assert!(
            (pse - 0.30).abs() < 0.02,
            "восстановленный PSE={pse}, истинный 0.30"
        );
    }

    #[test]
    fn biased_population_is_detected() {
        // Диверсия: если истинный PSE смещён к 0.24, пайплайн ОБЯЗАН это увидеть
        // (не залипнуть на 0.30). Это доказывает, что оценка следует за данными.
        let manifest = demo_manifest(202);
        let pop = Population {
            pse: 0.24,
            slope: -45.0,
            pse_sd: 0.02,
        };
        let sessions = simulate_population(&manifest, pop, 20, 424242);
        let pse = analysis::point_estimate(&sessions).expect("оценка PSE");
        assert!(
            (pse - 0.24).abs() < 0.02,
            "смещённый PSE={pse}, истинный 0.24"
        );
        assert!(pse < 0.28, "оценка не должна залипать на 0.30: {pse}");
    }
}
