//! Точное номинальное разбиение одного непрозрачного affine sRGB8-сегмента.
//!
//! Источник цвета — `a + t (b - a)` при общем вещественном `t ∈ [0, 1]`.
//! Для непрозрачных stops без transition hint и с явно выбранной интерполяцией
//! в encoded-sRGB (`in srgb`) это локальный закон CSS-градиента:
//! <https://www.w3.org/TR/css-images-3/#coloring-gradient-line> и
//! <https://www.w3.org/TR/css-color-4/#interpolation>.
//! Одна непрозрачность stops не определяет пространство интерполяции.
//! Округление здесь — выбранный Core nominal byte-law: nearest, half-up
//! (half-away на неотрицательных каналах), как в `composition.rs`. CSS не задаёт
//! этим законом точность rasterizer-а. Модуль не описывает renderer error,
//! прозрачность, multistop-поле или поиск назначений.
//!
//! Канал с `d = |b - a| > 0` пересекает ровно `d` half-integer порогов в точках
//! `(2j + 1) / (2d)`, `j = 0..d`. Между соседними точками объединения трёх
//! потоков ни один канал не может изменить byte. В самой точке возрастающий
//! канал уже принял верхний byte, а убывающий ещё сохраняет верхний: поэтому
//! точка может отличаться от ОБОИХ соседних интервалов. Точки не сливаются с
//! интервалами даже при совпадении цвета. Это покрытие всех вещественных `t`,
//! а не сетка, выборка или новый источник цвета вместо affine descriptor.

use std::cmp::Ordering;
use std::iter::FusedIterator;

/// Точный рациональный адрес границы в общей нормированной координате сегмента.
///
/// Координаты выдаёт разбиение; это не ограничение вещественной области поля.
/// У границы знаменатель ≤ 510, у внутреннего представителя интервала ≤ 1020.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SegmentCoordinate {
    numerator: u16,
    denominator: u16,
}

impl SegmentCoordinate {
    pub(crate) const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    pub(crate) const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// Все вызовы порождаются byte-порогом либо медиантой соседних границ.
    fn ratio(numerator: u16, denominator: u16) -> Self {
        debug_assert!(denominator > 0 && numerator <= denominator);
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self {
            numerator: numerator / a,
            denominator: denominator / a,
        }
    }

    pub(crate) const fn numerator(self) -> u16 {
        self.numerator
    }

    pub(crate) const fn denominator(self) -> u16 {
        self.denominator
    }
}

impl Ord for SegmentCoordinate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Даже произведение любых двух u16 помещается в u32; сравнение точное.
        (u32::from(self.numerator) * u32::from(other.denominator))
            .cmp(&(u32::from(other.numerator) * u32::from(self.denominator)))
    }
}

impl PartialOrd for SegmentCoordinate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Локальный непрозрачный encoded-sRGB сегмент; `t = 0` и `t = 1` — endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpaqueSrgb8AffineSegment {
    start: [u8; 3],
    end: [u8; 3],
}

impl OpaqueSrgb8AffineSegment {
    pub(crate) const fn new(start: [u8; 3], end: [u8; 3]) -> Self {
        Self { start, end }
    }

    pub(crate) const fn endpoints(self) -> ([u8; 3], [u8; 3]) {
        (self.start, self.end)
    }

    /// Точный nominal byte в рациональной координате, общей для всех каналов.
    pub(crate) fn at(self, coordinate: SegmentCoordinate) -> [u8; 3] {
        let numerator = u32::from(coordinate.numerator);
        let denominator = u32::from(coordinate.denominator);
        std::array::from_fn(|channel| {
            let value = u32::from(self.start[channel]) * (denominator - numerator)
                + u32::from(self.end[channel]) * numerator;
            // 0 ≤ value ≤ 255 * denominator. Даже при полном u16 denominator
            // `2 * value + denominator` ≤ 511 * 65535 < u32::MAX.
            ((2 * value + denominator) / (2 * denominator)) as u8
        })
    }

    pub(crate) fn partition(self) -> AffineSegmentPartition {
        AffineSegmentPartition {
            segment: self,
            crossings: self.crossings(),
            lower: SegmentCoordinate::ZERO,
            pending_point: Some(SegmentCoordinate::ZERO),
            finished: false,
        }
    }

    /// Точный размер для fallible reserve без вычисления RGB и медиант.
    /// `m` внутренних порогов разделяют `m + 1` интервалов и `m + 2` точек.
    #[cfg(test)]
    pub(crate) fn partition_cell_count(self) -> usize {
        2 * self.crossings().count() + 3
    }

    fn crossings(self) -> SegmentCrossings {
        SegmentCrossings {
            channels: std::array::from_fn(|channel| ChannelCrossings {
                distance: u16::from(self.start[channel].abs_diff(self.end[channel])),
                index: 0,
            }),
        }
    }
}

/// Описание области; границы открытого интервала не входят в него.
///
/// Само описание не удостоверяет, что границы выданы разбиением. Точный
/// представитель доступен только у `AffineSegmentCell`, созданной разбиением.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentRegion {
    Point(SegmentCoordinate),
    OpenInterval {
        lower: SegmentCoordinate,
        upper: SegmentCoordinate,
    },
}

impl SegmentRegion {
    fn representative(self) -> SegmentCoordinate {
        match self {
            Self::Point(coordinate) => coordinate,
            Self::OpenInterval { lower, upper } => {
                debug_assert!(lower < upper);
                // Медианта строго внутри (lower, upper). В отличие от midpoint,
                // знаменатели складываются: ≤ 510 + 510, без произведения.
                SegmentCoordinate::ratio(
                    lower.numerator + upper.numerator,
                    lower.denominator + upper.denominator,
                )
            }
        }
    }
}

/// Область постоянного nominal byte, созданная только `partition()`.
///
/// Закрытые поля не позволяют заменить границы координатами представителей:
/// медианта всегда складывает знаменатели двух границ, каждый не больше 510.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AffineSegmentCell {
    region: SegmentRegion,
    nominal: [u8; 3],
}

impl AffineSegmentCell {
    #[cfg(test)]
    pub(crate) const fn region(self) -> SegmentRegion {
        self.region
    }

    pub(crate) const fn nominal(self) -> [u8; 3] {
        self.nominal
    }

    /// Точный адрес для counterexample/replay; он не заменяет область покрытия.
    pub(crate) fn representative(self) -> SegmentCoordinate {
        self.region.representative()
    }
}

#[derive(Debug, Clone, Copy)]
struct ChannelCrossings {
    distance: u16,
    index: u16,
}

impl ChannelCrossings {
    fn peek(self) -> Option<SegmentCoordinate> {
        (self.index < self.distance)
            .then(|| SegmentCoordinate::ratio(2 * self.index + 1, 2 * self.distance))
    }
}

/// Одно слияние общих внутренних границ для подсчёта и построения областей.
#[derive(Debug, Clone)]
struct SegmentCrossings {
    channels: [ChannelCrossings; 3],
}

impl Iterator for SegmentCrossings {
    type Item = SegmentCoordinate;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self
            .channels
            .iter()
            .filter_map(|crossing| crossing.peek())
            .min()?;
        for crossing in &mut self.channels {
            if crossing.peek() == Some(next) {
                crossing.index += 1;
            }
        }
        Some(next)
    }
}

/// Ленивое слияние трёх потоков порогов без выделений памяти.
///
/// Descriptor сохраняется рядом с производным разбиением. Выход чередуется:
/// `{0}`, `(0, first)`, `{first}`, …, `(last, 1)`, `{1}`. Совпавшие пороги
/// разных каналов дают одну общую точку. При `m` различных внутренних порогов
/// выдаётся `2m + 3` областей; `m ≤ Σ|delta| ≤ 765`. Ранний выход не требует
/// вычислять или сохранять оставшееся разбиение.
#[derive(Debug, Clone)]
pub(crate) struct AffineSegmentPartition {
    segment: OpaqueSrgb8AffineSegment,
    crossings: SegmentCrossings,
    lower: SegmentCoordinate,
    pending_point: Option<SegmentCoordinate>,
    finished: bool,
}

impl Iterator for AffineSegmentPartition {
    type Item = AffineSegmentCell;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let region = if let Some(coordinate) = self.pending_point.take() {
            self.finished = coordinate == SegmentCoordinate::ONE;
            SegmentRegion::Point(coordinate)
        } else {
            let next = self.crossings.next().unwrap_or(SegmentCoordinate::ONE);
            let region = SegmentRegion::OpenInterval {
                lower: self.lower,
                upper: next,
            };
            self.lower = next;
            self.pending_point = Some(next);
            region
        };
        Some(AffineSegmentCell {
            region,
            nominal: self.segment.at(region.representative()),
        })
    }
}

impl FusedIterator for AffineSegmentPartition {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Независимое округление: найти первый НЕ достигнутый half-integer порог.
    /// u64 допускает проверочные рациональные адреса с большими знаменателями.
    fn oracle(segment: OpaqueSrgb8AffineSegment, numerator: u64, denominator: u64) -> [u8; 3] {
        assert!(denominator > 0 && numerator <= denominator);
        let (start, end) = segment.endpoints();
        std::array::from_fn(|channel| {
            let value = u64::from(start[channel]) * (denominator - numerator)
                + u64::from(end[channel]) * numerator;
            (0..=255)
                .find(|&byte| 2 * value < (2 * byte + 1) * denominator)
                .expect("an interpolated byte lies below the 255.5 upper threshold")
                as u8
        })
    }

    fn coordinate_oracle(segment: OpaqueSrgb8AffineSegment, at: SegmentCoordinate) -> [u8; 3] {
        oracle(
            segment,
            u64::from(at.numerator()),
            u64::from(at.denominator()),
        )
    }

    #[test]
    fn red_green_support_contains_all_511_bytes_and_keeps_tie_coordinates() {
        let segment = OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]);
        let partition = segment.partition();
        let cells: Vec<_> = partition.collect();
        assert_eq!(cells.len(), 513);
        let actual: BTreeSet<_> = cells.iter().map(|cell| cell.nominal).collect();
        let expected: BTreeSet<_> = (0..=255)
            .map(|k| [255 - k, k, 0])
            .chain((0..255).map(|k| [255 - k, k + 1, 0]))
            .collect();
        assert_eq!(actual.len(), 511);
        assert_eq!(actual, expected);
        let middle = cells
            .iter()
            .position(|cell| cell.region == SegmentRegion::Point(SegmentCoordinate::ratio(1, 2)))
            .expect("red-green interpolation has an exact half-coordinate cell");
        assert_eq!(cells[middle - 1].nominal, [128, 127, 0]);
        assert_eq!(cells[middle].nominal, [128, 128, 0]);
        assert_eq!(cells[middle + 1].nominal, [127, 128, 0]);
    }

    #[test]
    fn equal_endpoints_cover_the_whole_domain_and_iterator_is_fused() {
        let segment = OpaqueSrgb8AffineSegment::new([17, 128, 255], [17, 128, 255]);
        assert_eq!(segment.partition_cell_count(), 3);
        let mut partition = segment.partition();
        for region in [
            SegmentRegion::Point(SegmentCoordinate::ZERO),
            SegmentRegion::OpenInterval {
                lower: SegmentCoordinate::ZERO,
                upper: SegmentCoordinate::ONE,
            },
            SegmentRegion::Point(SegmentCoordinate::ONE),
        ] {
            assert_eq!(
                partition.next(),
                Some(AffineSegmentCell {
                    region,
                    nominal: [17, 128, 255],
                })
            );
        }
        assert_eq!(partition.next(), None);
        assert_eq!(partition.next(), None);
    }

    #[test]
    fn equal_monotone_channels_share_every_transition() {
        let segment = OpaqueSrgb8AffineSegment::new([0; 3], [255; 3]);
        let cells: Vec<_> = segment.partition().collect();
        assert_eq!(cells.len(), 513);
        for (index, cell) in cells.iter().enumerate() {
            assert_eq!(cell.nominal[0], cell.nominal[1]);
            assert_eq!(cell.nominal[1], cell.nominal[2]);
            if let SegmentRegion::Point(at) = cell.region {
                if at != SegmentCoordinate::ZERO && at != SegmentCoordinate::ONE {
                    assert_eq!(cell.nominal, cells[index + 1].nominal);
                    assert_eq!(cell.nominal[0], cells[index - 1].nominal[0] + 1);
                }
            }
        }
    }

    fn reverse_coordinate(at: SegmentCoordinate) -> SegmentCoordinate {
        SegmentCoordinate::ratio(at.denominator - at.numerator, at.denominator)
    }

    fn reverse_region(region: SegmentRegion) -> SegmentRegion {
        match region {
            SegmentRegion::Point(at) => SegmentRegion::Point(reverse_coordinate(at)),
            SegmentRegion::OpenInterval { lower, upper } => SegmentRegion::OpenInterval {
                lower: reverse_coordinate(upper),
                upper: reverse_coordinate(lower),
            },
        }
    }

    #[test]
    fn reversal_preserves_the_same_point_and_interval_values() {
        for (start, end) in [
            ([255, 0, 17], [0, 255, 17]),
            ([3, 200, 127], [254, 9, 126]),
            ([0, 1, 2], [255, 255, 255]),
            ([4, 4, 4], [4, 4, 4]),
        ] {
            let forward: Vec<_> = OpaqueSrgb8AffineSegment::new(start, end)
                .partition()
                .collect();
            let reversed: Vec<_> = OpaqueSrgb8AffineSegment::new(end, start)
                .partition()
                .collect();
            assert_eq!(forward.len(), reversed.len());
            for (left, right) in forward.iter().zip(reversed.iter().rev()) {
                assert_eq!(left.region, reverse_region(right.region));
                assert_eq!(left.nominal, right.nominal);
            }
        }
    }

    #[test]
    fn simultaneous_crossings_with_different_denominators_are_one_exact_point() {
        let segment = OpaqueSrgb8AffineSegment::new([0, 3, 11], [1, 0, 11]);
        let cells: Vec<_> = segment.partition().collect();
        // First channel crosses at 1/2, second at 1/6, 3/6, 5/6.
        assert_eq!(cells.len(), 9);
        assert_eq!(cells[3].nominal, [0, 2, 11]);
        assert_eq!(
            cells[4].region,
            SegmentRegion::Point(SegmentCoordinate::ratio(1, 2))
        );
        assert_eq!(cells[4].nominal, [1, 2, 11]);
        assert_eq!(cells[5].nominal, [1, 1, 11]);
    }

    /// Независимо перечислить пересечённые уровни каналов, затем проверить,
    /// что они ВСЕ и только они стали границами. Вместе с чередованием соседних
    /// границ это проверяет покрытие, которое не может доказать сетка адресов.
    fn check_partition(segment: OpaqueSrgb8AffineSegment) {
        let (start, end) = segment.endpoints();
        let mut thresholds = vec![(0_u64, 1_u64), (1, 1)];
        for channel in 0..3 {
            for byte in 0..255 {
                let mut numerator = 2 * byte + 1 - 2 * i32::from(start[channel]);
                let mut denominator = 2 * (i32::from(end[channel]) - i32::from(start[channel]));
                if denominator < 0 {
                    numerator = -numerator;
                    denominator = -denominator;
                }
                if numerator > 0 && numerator < denominator {
                    thresholds.push((numerator as u64, denominator as u64));
                }
            }
        }
        thresholds.sort_by(|(an, ad), (bn, bd)| (an * bd).cmp(&(bn * ad)));
        thresholds.dedup_by(|(an, ad), (bn, bd)| *an * *bd == *bn * *ad);
        let cells: Vec<_> = segment.partition().collect();
        assert_eq!(cells.len(), 2 * thresholds.len() - 1);
        assert_eq!(segment.partition_cell_count(), cells.len());
        for (index, &(numerator, denominator)) in thresholds.iter().enumerate() {
            let SegmentRegion::Point(at) = cells[2 * index].region() else {
                panic!("every boundary is a point");
            };
            assert_eq!(
                u64::from(at.numerator) * denominator,
                numerator * u64::from(at.denominator)
            );
            assert!(at.denominator <= 510);
            assert_eq!(cells[2 * index].representative(), at);
            assert_eq!(cells[2 * index].nominal(), coordinate_oracle(segment, at));
            if index + 1 == thresholds.len() {
                continue;
            }
            let SegmentRegion::OpenInterval { lower, upper } = cells[2 * index + 1].region() else {
                panic!("adjacent boundaries delimit an open interval");
            };
            assert_eq!(lower, at);
            assert_eq!(cells[2 * index + 2].region(), SegmentRegion::Point(upper));
            assert!(lower < upper);
            let representative = cells[2 * index + 1].representative();
            assert!(lower < representative && representative < upper);
            assert!(representative.denominator <= 1020);
            // Independent non-mediant rational interior points; includes addresses
            // whose denominator is much greater than the production coordinate.
            for weight in 1..5 {
                let n = (5 - weight) * u64::from(lower.numerator) * u64::from(upper.denominator)
                    + weight * u64::from(upper.numerator) * u64::from(lower.denominator);
                let d = 5 * u64::from(lower.denominator) * u64::from(upper.denominator);
                assert_eq!(cells[2 * index + 1].nominal(), oracle(segment, n, d));
            }
        }
    }

    #[test]
    fn all_byte_distances_and_directions_have_exact_partition_coverage() {
        for distance in 0..=255 {
            check_partition(OpaqueSrgb8AffineSegment::new(
                [0, distance, 255],
                [distance, 0, 255 - distance],
            ));
        }
    }

    #[test]
    fn mixed_channel_distances_and_offsets_match_independent_rational_oracle() {
        for (start, end) in [
            ([0, 1, 2], [255, 255, 255]),
            ([71, 247, 121], [252, 10, 119]),
            ([0, 254, 128], [255, 0, 129]),
            ([31, 63, 127], [223, 191, 128]),
            ([11, 90, 247], [78, 3, 21]),
            ([0, 255, 127], [1, 254, 127]),
        ] {
            check_partition(OpaqueSrgb8AffineSegment::new(start, end));
        }
    }

    #[test]
    fn lazy_iteration_preserves_descriptor_and_can_continue_from_a_clone() {
        let segment = OpaqueSrgb8AffineSegment::new([71, 247, 121], [252, 10, 119]);
        let mut partition = segment.partition();
        assert_eq!(
            partition
                .next()
                .expect("partition starts at its first endpoint")
                .nominal,
            segment.endpoints().0
        );
        assert!(matches!(
            partition
                .next()
                .expect("nonconstant partition continues with an interval")
                .region,
            SegmentRegion::OpenInterval { .. }
        ));
        let mut cloned = partition.clone();
        assert_eq!(partition.next(), cloned.next());
        assert_eq!(
            partition
                .last()
                .expect("partition ends at its second endpoint")
                .nominal,
            segment.endpoints().1
        );
    }
}
