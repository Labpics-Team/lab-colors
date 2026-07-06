//! Логистическая психометрика: MLE-подгонка `P(белый) = σ(a + b·Y)` руками (IRLS),
//! без тяжёлых зависимостей.
//!
//! Модель 2AFC: вероятность выбрать белый лейбл падает с ростом люминанс свотча,
//! поэтому наклон `b` отрицателен. Точка субъективного равенства (PSE) —
//! `Y*`, где `P = 0.5`, то есть `a + b·Y* = 0 ⇒ Y* = −a/b`. Это и есть кроссовер
//! полярности, оценка `PAIR_CROSSOVER_Y`.
//!
//! Подгонка — Ньютон/IRLS (`θ ← θ + (XᵀWX)⁻¹ Xᵀ(y−p)`) с крошечным L2-гребнем
//! `λ`, который держит `XᵀWX` обратимой при почти-разделяющих ресэмплах bootstrap
//! (иначе `b → ∞`). `λ = 1e-6` сдвигает оценку ниже численного шума, но убирает
//! вырождение — доказано тестом `ridge_barely_perturbs_estimate`.

/// Одна точка данных: люминанс `y` и бинарный отклик «выбран белый».
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub y: f64,
    pub chose_white: bool,
}

/// Результат подгонки.
#[derive(Debug, Clone, Copy)]
pub struct Fit {
    /// Свободный член.
    pub a: f64,
    /// Наклон (для валидных данных отрицателен).
    pub b: f64,
    /// Сошёлся ли Ньютон в пределах допуска.
    pub converged: bool,
    /// Число итераций.
    pub iters: u32,
}

const RIDGE: f64 = 1e-6;
const MAX_ITERS: u32 = 100;
const TOL: f64 = 1e-11;

/// Логистическая сигмоида, численно устойчивая на больших `|z|`.
#[must_use]
pub fn sigmoid(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

/// Подогнать логистику IRLS. Возвращает `None`, если точек < 2 или в них нет
/// обоих исходов при одной люминанс (модель невырождаема лишь при вариации).
#[must_use]
pub fn fit(points: &[Point]) -> Option<Fit> {
    if points.len() < 2 {
        return None;
    }
    // Требуем вариацию по Y и хотя бы оба исхода — иначе наклон не определён.
    let first_y = points[0].y;
    let y_varies = points.iter().any(|p| (p.y - first_y).abs() > 1e-12);
    let any_white = points.iter().any(|p| p.chose_white);
    let any_ink = points.iter().any(|p| !p.chose_white);
    if !y_varies || !any_white || !any_ink {
        return None;
    }

    // Старт: a=0, b=0 (p=0.5 всюду).
    let mut a = 0.0f64;
    let mut b = 0.0f64;
    let mut converged = false;
    let mut used = MAX_ITERS;

    for it in 0..MAX_ITERS {
        used = it + 1;
        let (mut ga, mut gb) = (0.0f64, 0.0f64);
        let (mut haa, mut hab, mut hbb) = (0.0f64, 0.0f64, 0.0f64);
        for p in points {
            let z = a + b * p.y;
            let mu = sigmoid(z);
            let target = if p.chose_white { 1.0 } else { 0.0 };
            let r = target - mu;
            ga += r;
            gb += r * p.y;
            let w = mu * (1.0 - mu);
            haa += w;
            hab += w * p.y;
            hbb += w * p.y * p.y;
        }
        // L2-гребень: −λθ в градиент, +λ на диагональ гессиана.
        ga -= RIDGE * a;
        gb -= RIDGE * b;
        haa += RIDGE;
        hbb += RIDGE;

        let det = haa * hbb - hab * hab;
        if det.abs() < 1e-18 || !det.is_finite() {
            break;
        }
        let da = (hbb * ga - hab * gb) / det;
        let db = (-hab * ga + haa * gb) / det;
        if !da.is_finite() || !db.is_finite() {
            break;
        }
        a += da;
        b += db;
        if da.abs() < TOL && db.abs() < TOL {
            converged = true;
            break;
        }
    }

    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    Some(Fit {
        a,
        b,
        converged,
        iters: used,
    })
}

/// PSE (`Y*` при `P=0.5`) из подгонки: `−a/b`. `None`, если наклон ~0.
/// Сходимость НЕ проверяет — вызывающий смотрит `fit.converged` (см. `fit_pse`).
#[must_use]
pub fn pse(fit: &Fit) -> Option<f64> {
    if fit.b.abs() < 1e-9 {
        return None;
    }
    let v = -fit.a / fit.b;
    v.is_finite().then_some(v)
}

/// Удобно: подогнать и вернуть PSE.
///
/// `None`, если данные вырождены ИЛИ Ньютон не сошёлся: несошедшиеся `a,b` —
/// потенциально смещённая оценка, молча доверять ей нельзя. Несошедшийся
/// bootstrap-ресэмпл при этом просто выбрасывается из CI (см. `analysis`).
#[must_use]
pub fn fit_pse(points: &[Point]) -> Option<f64> {
    let f = fit(points)?;
    if !f.converged {
        return None;
    }
    pse(&f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;

    /// Сгенерировать отклики истинной логистикой с известными a,b.
    fn synth(a: f64, b: f64, ys: &[f64], reps: usize, seed: u64) -> Vec<Point> {
        let mut rng = SplitMix64::new(seed);
        let mut pts = Vec::new();
        for &y in ys {
            for _ in 0..reps {
                let p = sigmoid(a + b * y);
                pts.push(Point {
                    y,
                    chose_white: rng.next_f64() < p,
                });
            }
        }
        pts
    }

    #[test]
    fn sigmoid_basic() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-12);
        assert!(sigmoid(50.0) > 0.999_999);
        assert!(sigmoid(-50.0) < 1e-6);
        // Устойчивость: без NaN на экстремумах.
        assert!(sigmoid(1000.0).is_finite());
        assert!(sigmoid(-1000.0).is_finite());
    }

    #[test]
    fn recovers_known_pse() {
        // Истинный PSE = 0.30: a + b*0.30 = 0. Возьмём b=-40 (крутой), a=12.
        let a_true = 12.0;
        let b_true = -40.0;
        let ys: Vec<f64> = (0..14).map(|k| 0.18 + 0.02 * f64::from(k)).collect();
        let pts = synth(a_true, b_true, &ys, 400, 20250706);
        let f = fit(&pts).expect("подгонка");
        let pse_hat = pse(&f).expect("pse");
        assert!(f.converged, "IRLS сошёлся");
        assert!((pse_hat - 0.30).abs() < 0.01, "pse_hat={pse_hat}");
        assert!(f.b < 0.0, "наклон отрицателен: {}", f.b);
    }

    #[test]
    fn degenerate_data_returns_none() {
        // Все отклики одинаковы → нет обоих исходов.
        let pts: Vec<Point> = (0..10)
            .map(|k| Point {
                y: 0.18 + 0.02 * f64::from(k),
                chose_white: true,
            })
            .collect();
        assert!(fit(&pts).is_none());
        // Нет вариации по Y.
        let flat: Vec<Point> = (0..10)
            .map(|i| Point {
                y: 0.3,
                chose_white: i % 2 == 0,
            })
            .collect();
        assert!(fit(&flat).is_none());
    }

    #[test]
    fn ridge_barely_perturbs_estimate() {
        // На хорошо-обусловленных данных гребень 1e-6 не сдвигает PSE значимо:
        // сравниваем с крупной выборкой (правда близка к 0.30).
        let ys: Vec<f64> = (0..14).map(|k| 0.18 + 0.02 * f64::from(k)).collect();
        let pts = synth(12.0, -40.0, &ys, 2000, 7);
        let pse_hat = fit_pse(&pts).expect("pse");
        assert!((pse_hat - 0.30).abs() < 0.005, "pse_hat={pse_hat}");
    }

    #[test]
    fn too_few_points_none() {
        assert!(
            fit(&[Point {
                y: 0.2,
                chose_white: true
            }])
            .is_none()
        );
        assert!(fit(&[]).is_none());
    }
}
