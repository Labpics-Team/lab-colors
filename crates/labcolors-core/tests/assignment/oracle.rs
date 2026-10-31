//! Независимые перебор, threshold arithmetic и Oklab-транскрипция для пробы.
//! Нормативные Q55-данные разделяются с Core и отдельно проверяются Python verifier.

use super::experiment::{Constraint, Domain, Floor, Problem, Rgb, Source};
use std::sync::OnceLock;

const DATA: &[u8] = include_bytes!("../../contracts/wcag22-srgb8-q55-v1.bin");
const SCALE: u128 = 1 << 55;
type Table = [[[u64; 2]; 256]; 3];

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        assert_eq!(DATA.len(), 3 * 256 * 2 * 8);
        std::array::from_fn(|channel| {
            std::array::from_fn(|code| {
                std::array::from_fn(|end| {
                    let offset = ((channel * 256 + code) * 2 + end) * 8;
                    u64::from_le_bytes(
                        DATA[offset..offset + 8]
                            .try_into()
                            .expect("проверен размер artifact"),
                    )
                })
            })
        })
    })
}

fn y(rgb: Rgb) -> [u128; 2] {
    std::array::from_fn(|end| {
        (0..3)
            .map(|channel| u128::from(table()[channel][usize::from(rgb[channel])][end]))
            .sum()
    })
}

fn source_value(source: Source, assignment: &[Rgb]) -> Rgb {
    match source {
        Source::Fixed(rgb) => rgb,
        Source::Target(i) => assignment[i],
    }
}

fn holds(problem: &Problem, assignment: &[Rgb]) -> bool {
    for predicate in &problem.constraints {
        match *predicate {
            Constraint::Contrast {
                lighter,
                darker,
                floor,
            } => {
                let lo = y(source_value(lighter, assignment))[0];
                let hi = y(source_value(darker, assignment))[1];
                let (num, den) = match floor {
                    Floor::Three => (3, 1),
                    Floor::FourAndHalf => (9, 2),
                };
                // Независимая форма: (L + 1/20) / (D + 1/20) >= num/den.
                if (20 * lo + SCALE) * den < (20 * hi + SCALE) * num {
                    return false;
                }
            }
            Constraint::Gap {
                lighter,
                darker,
                minimum_q55,
            } => {
                if y(source_value(lighter, assignment))[0]
                    < y(source_value(darker, assignment))[1] + u128::from(minimum_q55)
                {
                    return false;
                }
            }
        }
    }
    true
}

pub fn lab(rgb: Rgb) -> [f64; 3] {
    // Ottosson 2021-01-25; независимая скалярная форма без вызовов Core.
    let [r, g, b] = rgb.map(|value| {
        let v = f64::from(value) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

pub fn delta(a: Rgb, b: Rgb) -> f64 {
    let a = lab(a);
    let b = lab(b);
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[derive(Debug)]
pub struct OracleRun {
    pub best: Option<Vec<Rgb>>,
    pub visited: u64,
    pub feasible: u64,
}

struct Enumeration<'a> {
    problem: &'a Problem,
    assignment: Vec<Rgb>,
    best: Option<(Vec<f64>, Vec<Rgb>)>,
    visited: u64,
    feasible: u64,
}

impl Enumeration<'_> {
    fn consider(&mut self) {
        self.visited += 1;
        if !holds(self.problem, &self.assignment) {
            return;
        }
        let mut rank = Vec::with_capacity(self.assignment.len());
        for (i, target) in self.problem.targets.iter().enumerate() {
            let rgb = self.assignment[i];
            let (lower, upper) = target.domain.bounds();
            if (0..3).any(|j| rgb[j] < lower[j] || rgb[j] > upper[j]) {
                return;
            }
            let difference = delta(rgb, target.anchor);
            if difference > target.max_delta {
                return;
            }
            rank.push(difference);
        }
        self.feasible += 1;
        let improves = match &self.best {
            None => true,
            Some((old_rank, old_colors)) => {
                rank < *old_rank || rank == *old_rank && self.assignment < *old_colors
            }
        };
        if improves {
            self.best = Some((rank, self.assignment.clone()));
        }
    }

    fn enumerate(&mut self, i: usize) {
        match self.problem.targets[i].domain {
            Domain::Gray { lower, upper } => {
                for value in u16::from(lower)..=u16::from(upper) {
                    self.assignment[i] = [value as u8; 3];
                    self.next(i);
                }
            }
            Domain::Cube { .. } => {
                // Полный sRGB8 oracle не использует block/pruning candidate.
                for word in 0u32..1 << 24 {
                    self.assignment[i] = [(word >> 16) as u8, (word >> 8) as u8, word as u8];
                    self.next(i);
                }
            }
        }
    }

    fn next(&mut self, i: usize) {
        if i + 1 == self.assignment.len() {
            self.consider();
        } else {
            self.enumerate(i + 1);
        }
    }
}

pub fn solve(problem: &Problem) -> OracleRun {
    let mut run = Enumeration {
        problem,
        assignment: vec![[0; 3]; problem.targets.len()],
        best: None,
        visited: 0,
        feasible: 0,
    };
    run.enumerate(0);
    OracleRun {
        best: run.best.map(|(_, values)| values),
        visited: run.visited,
        feasible: run.feasible,
    }
}
