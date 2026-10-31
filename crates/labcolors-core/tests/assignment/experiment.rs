//! Только исследовательский integration-test, без shipping entry point.

use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, Wcag22LuminanceBoundsQ55V1,
    evaluate_wcag22_srgb8,
};
use labcolors_core::{Srgb8, oklch_from_hex};
use std::cmp::Ordering;

pub type Rgb = [u8; 3];
type Bounds = (u64, u64);

#[derive(Clone, Copy, Debug)]
pub enum Domain {
    Cube { lower: Rgb, upper: Rgb },
    Gray { lower: u8, upper: u8 },
}

impl Domain {
    pub const fn full() -> Self {
        Self::Cube {
            lower: [0; 3],
            upper: [255; 3],
        }
    }

    pub fn bounds(self) -> (Rgb, Rgb) {
        match self {
            Self::Cube { lower, upper } => (lower, upper),
            Self::Gray { lower, upper } => ([lower; 3], [upper; 3]),
        }
    }

    pub fn contains(self, rgb: Rgb) -> bool {
        let (lower, upper) = self.bounds();
        (0..3).all(|i| lower[i] <= rgb[i] && rgb[i] <= upper[i])
            && (!matches!(self, Self::Gray { .. }) || rgb[0] == rgb[1] && rgb[1] == rgb[2])
    }
}

#[derive(Clone, Debug)]
pub struct Target {
    pub id: &'static str,
    pub anchor: Rgb,
    pub domain: Domain,
    pub max_delta: f64,
}

#[derive(Clone, Copy, Debug)]
pub enum Source {
    Fixed(Rgb),
    Target(usize),
}

impl Source {
    pub fn rgb(self, values: &[Rgb]) -> Rgb {
        match self {
            Self::Fixed(rgb) => rgb,
            Self::Target(index) => values[index],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Floor {
    Three,
    FourAndHalf,
}

impl Floor {
    fn criterion(self) -> Wcag22CriterionV1 {
        match self {
            Self::Three => Wcag22CriterionV1::Sc143TextLargeScale,
            Self::FourAndHalf => Wcag22CriterionV1::Sc143TextDefault,
        }
    }

    /// Для Q55-яркостей L, D и S = 2^55 условие WCAG с порогом p/q:
    /// (L/S + 1/20) / (D/S + 1/20) >= p/q, то есть
    /// 20qL >= 20pD + (p-q)S. При 3:1 сокращаем (20, 60, 2)
    /// до (10, 30, 1); при 9:2 получаем (40, 180, 7).
    /// Пороги совпадают с criterion; oracle выводит их независимо.
    pub fn coefficients(self) -> (u128, u128, u128) {
        match self {
            Self::Three => (10, 30, 1),
            Self::FourAndHalf => (40, 180, 7),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Constraint {
    Contrast {
        lighter: Source,
        darker: Source,
        floor: Floor,
    },
    Gap {
        lighter: Source,
        darker: Source,
        minimum_q55: u64,
    },
}

#[derive(Clone, Debug)]
pub struct Problem {
    // Порядок целей является явным порядком предпочтений. ID не участвует в решении.
    pub targets: Vec<Target>,
    pub constraints: Vec<Constraint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Assigned(Vec<Rgb>),
    Unsatisfied,
    Unresolved { incumbent: Option<Vec<Rgb>> },
}

#[derive(Debug)]
pub struct Run {
    pub outcome: Outcome,
    pub examined: u64,
    pub pruned: u64,
}

fn bounds(rgb: Rgb) -> Bounds {
    let assessment = evaluate_wcag22_srgb8(rgb, [255; 3], Wcag22CriterionV1::Sc143TextDefault)
        .expect("валидная sRGB8-пара поддерживается существующим proof-bound evaluator");
    match assessment {
        Wcag22AssessmentV1::Evaluated { measurement, .. } => (
            measurement.foreground_luminance.lower(),
            measurement.foreground_luminance.upper(),
        ),
        _ => panic!("sRGB8 evaluator обязан вернуть измеренный результат"),
    }
}

fn lab(rgb: Rgb) -> [f64; 3] {
    let [l, c, h] =
        oklch_from_hex(&Srgb8::new(rgb).to_hex()).expect("Srgb8 сериализуется в корректный hex");
    let (sin, cos) = h.to_radians().sin_cos();
    [l, c * cos, c * sin]
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn constraints_hold(
    problem: &Problem,
    values: &[Rgb],
    mut gap_bounds: impl FnMut(Source, usize, usize) -> Bounds,
) -> bool {
    for (index, constraint) in problem.constraints.iter().enumerate() {
        match *constraint {
            Constraint::Contrast {
                lighter,
                darker,
                floor,
            } => {
                let lighter = lighter.rgb(values);
                let darker = darker.rgb(values);
                // Один assessment содержит обе яркости. WCAG симметричен,
                // поэтому авторская полярность остаётся отдельным условием.
                let assessment = evaluate_wcag22_srgb8(lighter, darker, floor.criterion())
                    .expect("валидная sRGB8-пара имеет полный нормативный verdict");
                if !matches!(
                    assessment,
                    Wcag22AssessmentV1::Evaluated {
                        decision: Wcag22ApplicableDecisionV1::Pass,
                        measurement,
                        ..
                    } if measurement.foreground_luminance.lower()
                        > measurement.background_luminance.upper()
                ) {
                    return false;
                }
            }
            Constraint::Gap {
                lighter,
                darker,
                minimum_q55,
            } => {
                if u128::from(gap_bounds(lighter, index, 0).0)
                    < u128::from(gap_bounds(darker, index, 1).1) + u128::from(minimum_q55)
                {
                    return false;
                }
            }
        }
    }
    true
}

pub fn admissible(problem: &Problem, values: &[Rgb]) -> bool {
    constraints_hold(problem, values, |source, _, _| bounds(source.rgb(values)))
        && problem.targets.iter().zip(values).all(|(target, &rgb)| {
            target.domain.contains(rgb)
                && distance_squared(lab(rgb), lab(target.anchor)) <= target.max_delta.powi(2)
        })
}

pub struct Search<'a> {
    problem: &'a Problem,
    anchor_lab: Vec<[f64; 3]>,
    values: Vec<Rgb>,
    constraint_bounds: Vec<[Bounds; 2]>,
    value_bounds: Vec<Option<Bounds>>,
    scores: Vec<f64>,
    scored: Vec<bool>,
    best: Option<(Vec<f64>, Vec<Rgb>)>,
    budget: u64,
    examined: u64,
    pruned: u64,
    exhausted: bool,
    pruning: bool,
}

impl Search<'_> {
    fn examine(&mut self) {
        if self.examined == self.budget {
            self.exhausted = true;
            return;
        }
        self.examined += 1;
        let values = &self.values;
        let value_bounds = &mut self.value_bounds;
        if !constraints_hold(
            self.problem,
            values,
            |source, constraint, side| match source {
                Source::Fixed(_) => self.constraint_bounds[constraint][side],
                Source::Target(index) => {
                    *value_bounds[index].get_or_insert_with(|| bounds(values[index]))
                }
            },
        ) {
            return;
        }
        // Назначенный префикс не меняется при обходе суффикса. Ленивое обновление
        // сохраняет отсутствие Lab-работы на листьях, отвергнутых constraints.
        for (index, scored) in self.scored.iter_mut().enumerate() {
            if !*scored {
                self.scores[index] =
                    distance_squared(lab(self.values[index]), self.anchor_lab[index]);
                *scored = true;
            }
        }
        if !self
            .problem
            .targets
            .iter()
            .zip(&self.values)
            .zip(&self.scores)
            .all(|((target, &rgb), &score)| {
                target.domain.contains(rgb) && score <= target.max_delta.powi(2)
            })
        {
            return;
        }
        let replace = self.best.as_ref().is_none_or(|(best, colors)| {
            let order = self
                .scores
                .iter()
                .zip(best)
                .map(|(a, b)| a.total_cmp(b))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal);
            order == Ordering::Less || order == Ordering::Equal && self.values < *colors
        });
        if replace {
            if let Some((scores, values)) = &mut self.best {
                scores.copy_from_slice(&self.scores);
                values.copy_from_slice(&self.values);
            } else {
                self.best = Some((self.scores.clone(), self.values.clone()));
            }
        }
    }

    fn possible(&mut self, index: usize, lower: Rgb, upper: Rgb) -> bool {
        let mut current = None;
        let mut interval = |source: Source, constraint: usize, side: usize| {
            let (low, high) = match source {
                Source::Target(i) if i < index => {
                    *self.value_bounds[i].get_or_insert_with(|| bounds(self.values[i]))
                }
                Source::Target(i) if i == index => {
                    *current.get_or_insert_with(|| (bounds(lower).0, bounds(upper).1))
                }
                _ => self.constraint_bounds[constraint][side],
            };
            (u128::from(low), u128::from(high))
        };
        self.problem
            .constraints
            .iter()
            .enumerate()
            .all(|(index, constraint)| match *constraint {
                Constraint::Contrast {
                    lighter,
                    darker,
                    floor,
                } => {
                    let (light_factor, dark_factor, offset) = floor.coefficients();
                    light_factor * interval(lighter, index, 0).1
                        >= dark_factor * interval(darker, index, 1).0
                            + offset * u128::from(Wcag22LuminanceBoundsQ55V1::scale())
                }
                Constraint::Gap {
                    lighter,
                    darker,
                    minimum_q55,
                } => {
                    interval(lighter, index, 0).1
                        >= interval(darker, index, 1).0 + u128::from(minimum_q55)
                }
            })
    }

    fn block(&mut self, index: usize, lower: Rgb, upper: Rgb, gray: bool) {
        if self.pruning && !self.possible(index, lower, upper) {
            self.pruned += u64::from(upper[2]) - u64::from(lower[2]) + 1;
            return;
        }
        for blue in u16::from(lower[2])..=u16::from(upper[2]) {
            self.values[index] = if gray {
                [blue as u8; 3]
            } else {
                [lower[0], lower[1], blue as u8]
            };
            self.value_bounds[index] = None;
            self.scored[index] = false;
            if index + 1 == self.values.len() {
                self.examine();
            } else {
                self.visit(index + 1);
            }
            if self.exhausted {
                return;
            }
        }
    }

    fn visit(&mut self, index: usize) {
        match self.problem.targets[index].domain {
            Domain::Gray { lower, upper } => self.block(index, [lower; 3], [upper; 3], true),
            Domain::Cube { lower, upper } => {
                for red in u16::from(lower[0])..=u16::from(upper[0]) {
                    for green in u16::from(lower[1])..=u16::from(upper[1]) {
                        self.block(
                            index,
                            [red as u8, green as u8, lower[2]],
                            [red as u8, green as u8, upper[2]],
                            false,
                        );
                        if self.exhausted {
                            return;
                        }
                    }
                }
            }
        }
    }

    pub fn run(mut self) -> Run {
        self.examine();
        // Нулевая цель предпочтения глобально минимальна: это общий закон, не recipe.
        if self.best.is_some() {
            return Run {
                outcome: Outcome::Assigned(self.values),
                examined: self.examined,
                pruned: 0,
            };
        }
        if !self.exhausted {
            self.visit(0);
        }
        let best = self.best.map(|(_, values)| values);
        let outcome = if self.exhausted {
            Outcome::Unresolved { incumbent: best }
        } else {
            best.map_or(Outcome::Unsatisfied, Outcome::Assigned)
        };
        Run {
            outcome,
            examined: self.examined,
            pruned: self.pruned,
        }
    }
}

pub fn prepare(problem: &Problem, pruning: bool, budget: u64) -> Search<'_> {
    assert!(
        !problem.targets.is_empty(),
        "протокол пробы требует хотя бы одну цель"
    );
    assert!(
        problem
            .targets
            .iter()
            .all(|t| !t.id.is_empty() && t.max_delta.is_finite() && t.max_delta >= 0.0)
    );
    let domain_bounds: Vec<_> = problem
        .targets
        .iter()
        .map(|target| {
            let (lower, upper) = target.domain.bounds();
            (bounds(lower).0, bounds(upper).1)
        })
        .collect();
    let prepared_bounds = |source| match source {
        Source::Fixed(rgb) => bounds(rgb),
        Source::Target(index) => domain_bounds[index],
    };
    Search {
        problem,
        anchor_lab: problem.targets.iter().map(|t| lab(t.anchor)).collect(),
        values: problem.targets.iter().map(|t| t.anchor).collect(),
        constraint_bounds: problem
            .constraints
            .iter()
            .map(|constraint| {
                let (Constraint::Contrast {
                    lighter, darker, ..
                }
                | Constraint::Gap {
                    lighter, darker, ..
                }) = *constraint;
                [prepared_bounds(lighter), prepared_bounds(darker)]
            })
            .collect(),
        value_bounds: vec![None; problem.targets.len()],
        scores: vec![0.0; problem.targets.len()],
        scored: vec![false; problem.targets.len()],
        best: None,
        budget,
        examined: 0,
        pruned: 0,
        exhausted: false,
        pruning,
    }
}

pub fn solve(problem: &Problem, pruning: bool, budget: u64) -> Run {
    prepare(problem, pruning, budget).run()
}
