//! Точная package-pinned конвенция над финальными encoded-sRGB8 байтами.
//!
//! Выпуск v2 связывает условную дискретную номинальную модель и полный
//! воспроизводимый вывод. Predicate включается только явным constraint Program;
//! человеческий допуск или право автоматической публикации из него не следуют.

use crate::Srgb8;

const CODEC_BYTES: usize = 11_280;
const CODEC_HEADER_BYTES: usize = 8;
const CODEC_OFFSET_COUNT: usize = 257;
const CODEC_OFFSET_BYTES: usize = CODEC_OFFSET_COUNT * 2;
const CODEC_BODY_OFFSET: usize = CODEC_HEADER_BYTES + CODEC_OFFSET_BYTES;
const CODEC_RECORD_BYTES: usize = 3;

// Точный размер в типе превращает усечение или добавление байтов package-data
// в compile error до того, как lookup сможет увидеть повреждённый индекс.
const CODEC: &[u8; CODEC_BYTES] =
    include_bytes!("../contracts/clean-set-srgb8-v2/point-clean-set-srgb8-column-rle-v1.bin");

const _: () = {
    // Развёрнутые проверки делают каждый байт независимым compile-time
    // обязательством: ослабление границы цикла не может незаметно сократить
    // проверяемый префикс.
    assert!(CODEC[0] == b'L');
    assert!(CODEC[1] == b'P');
    assert!(CODEC[2] == b'C');
    assert!(CODEC[3] == b'C');
    assert!(CODEC[4] == 1);
    assert!(CODEC[5] == 1);
    assert!(CODEC[6] == 0);
    assert!(CODEC[7] == 0);
};

#[cfg(test)]
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_ACCEPTED_COUNT_V2: u32 = 8_342_111;
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2: [u8; 32] = [
    0xcb, 0x3a, 0x87, 0x81, 0x63, 0xc3, 0x07, 0x02, 0xde, 0xf5, 0xa2, 0x3d, 0xc2, 0xba, 0xf1, 0x4c,
    0x7b, 0xed, 0x02, 0x2c, 0x99, 0xe4, 0x3a, 0xf4, 0x07, 0xbf, 0xd8, 0xde, 0xd9, 0x2c, 0xac, 0x53,
];
#[cfg(test)]
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_CODEC_SHA256_V2: [u8; 32] = [
    0xad, 0x4a, 0x72, 0xb6, 0xe3, 0xb1, 0x89, 0x50, 0xf3, 0x85, 0x44, 0xd1, 0x99, 0x98, 0x05, 0x2a,
    0xbf, 0x9a, 0xb6, 0xe1, 0x23, 0x87, 0x61, 0x3e, 0xed, 0x4a, 0xd0, 0xd8, 0x3d, 0xfa, 0x4a, 0x8f,
];
#[cfg(test)]
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_RAW_TABLE_SHA256_V2: [u8; 32] = [
    0xcf, 0x42, 0x41, 0x99, 0x77, 0x85, 0x0c, 0x43, 0x5e, 0xde, 0x3e, 0x5b, 0xe0, 0x5a, 0x0a, 0x74,
    0xb1, 0x4a, 0xd9, 0x82, 0x55, 0x41, 0x8a, 0xac, 0x7d, 0x39, 0x6f, 0x5b, 0xb5, 0x01, 0x55, 0x0e,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RejectedBlueIntervalV1 {
    None,
    Closed { lo: u8, hi: u8 },
}

impl RejectedBlueIntervalV1 {
    #[cfg(test)]
    pub(crate) const fn contains_closed(self, blue: u8) -> bool {
        match self {
            Self::None => false,
            Self::Closed { lo, hi } => lo <= blue && blue <= hi,
        }
    }

    #[cfg(test)]
    pub(crate) const fn raw_pair_v1(self) -> [u8; 2] {
        match self {
            Self::None => [u8::MAX, 0],
            Self::Closed { lo, hi } => [lo, hi],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactNominalSrgb8CleanSetDecisionV1 {
    Accepted,
    Rejected(ClosedRejectedBlueIntervalV1),
}

/// Непустой closed interval отделён от table-sentinel типом: evidence
/// `Rejected(None)` невозможно собрать даже внутри соседнего модуля.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClosedRejectedBlueIntervalV1 {
    lo: u8,
    hi: u8,
}

impl ClosedRejectedBlueIntervalV1 {
    const fn from_canonical_table(lo: u8, hi: u8) -> Self {
        Self { lo, hi }
    }

    pub(crate) const fn endpoints(self) -> [u8; 2] {
        [self.lo, self.hi]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ExactNominalSrgb8CleanSetV2;

impl ExactNominalSrgb8CleanSetV2 {
    pub(crate) fn classify(self, color: Srgb8) -> ExactNominalSrgb8CleanSetDecisionV1 {
        // Нейтральная ось входит в declared set отдельным exact union: таблица
        // описывает только chromatic complement и не вправе её исключить.
        if color.is_achromatic() {
            return ExactNominalSrgb8CleanSetDecisionV1::Accepted;
        }

        let [red, green, blue] = color.bytes();
        let interval = self.rejected_blue_interval(red, green);
        match interval {
            RejectedBlueIntervalV1::Closed { lo, hi } if lo <= blue && blue <= hi => {
                ExactNominalSrgb8CleanSetDecisionV1::Rejected(
                    ClosedRejectedBlueIntervalV1::from_canonical_table(lo, hi),
                )
            }
            RejectedBlueIntervalV1::None | RejectedBlueIntervalV1::Closed { .. } => {
                ExactNominalSrgb8CleanSetDecisionV1::Accepted
            }
        }
    }

    pub(crate) fn rejected_blue_interval(self, red: u8, green: u8) -> RejectedBlueIntervalV1 {
        let column = usize::from(green);
        let mut lower = usize::from(codec_offset(column));
        let mut upper = usize::from(codec_offset(column + 1));

        // Каждый content-bound column непуст и начинается с red=0. Ищем
        // последний run start, не превосходящий вход: максимум семь probes.
        while lower + 1 < upper {
            let middle = lower + (upper - lower) / 2;
            if codec_record(middle)[0] <= red {
                lower = middle;
            } else {
                upper = middle;
            }
        }

        let [_red_start, lo, hi] = codec_record(lower);
        match [lo, hi] {
            [u8::MAX, 0] => RejectedBlueIntervalV1::None,
            [lo, hi] => RejectedBlueIntervalV1::Closed { lo, hi },
        }
    }
}

#[cfg(test)]
pub(crate) const fn exact_nominal_srgb8_clean_set_codec_v1() -> &'static [u8; CODEC_BYTES] {
    CODEC
}

fn codec_offset(index: usize) -> u16 {
    let byte = CODEC_HEADER_BYTES + index * 2;
    u16::from_be_bytes([CODEC[byte], CODEC[byte + 1]])
}

fn codec_record(index: usize) -> [u8; CODEC_RECORD_BYTES] {
    let byte = CODEC_BODY_OFFSET + index * CODEC_RECORD_BYTES;
    [CODEC[byte], CODEC[byte + 1], CODEC[byte + 2]]
}
