//! Точная package-pinned конвенция над финальными encoded-sRGB8 байтами.
//!
//! Модуль намеренно остаётся внутренним: R3a добавляет один predicate в
//! canonical Program, но ещё не публикует `auto`, writer или отдельный quality
//! façade.

use crate::Srgb8;

const CODEC_BYTES: usize = 11_370;
const CODEC_HEADER_BYTES: usize = 8;
const CODEC_OFFSET_COUNT: usize = 257;
const CODEC_OFFSET_BYTES: usize = CODEC_OFFSET_COUNT * 2;
const CODEC_BODY_OFFSET: usize = CODEC_HEADER_BYTES + CODEC_OFFSET_BYTES;
const CODEC_RECORD_BYTES: usize = 3;

// Точный размер в типе превращает усечение или добавление байтов package-data
// в compile error до того, как lookup сможет увидеть повреждённый индекс.
const CODEC: &[u8; CODEC_BYTES] =
    include_bytes!("../contracts/clean-set-srgb8-v1/point-clean-set-srgb8-column-rle-v1.bin");

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
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_ACCEPTED_COUNT_V1: u32 = 8_232_849;
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1: [u8; 32] = [
    0x67, 0xca, 0xda, 0xae, 0x38, 0xbb, 0xae, 0xa3, 0x09, 0x6d, 0xba, 0x69, 0x14, 0x2b, 0x5b, 0xf3,
    0xd7, 0x77, 0x6b, 0x75, 0x74, 0xec, 0x22, 0x40, 0x22, 0xab, 0xbc, 0xd1, 0x19, 0xc4, 0x5c, 0xe6,
];
#[cfg(test)]
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_CODEC_SHA256_V1: [u8; 32] = [
    0xaa, 0x6a, 0xa7, 0xc0, 0xb6, 0x30, 0x43, 0x7f, 0x1c, 0x1b, 0xa8, 0xc2, 0xce, 0xaf, 0xb0, 0xda,
    0xdf, 0x65, 0x51, 0xc4, 0x23, 0x31, 0x55, 0x95, 0x04, 0x07, 0x6a, 0x6c, 0xd4, 0x4e, 0x63, 0x31,
];
#[cfg(test)]
pub(crate) const EXACT_NOMINAL_SRGB8_CLEAN_SET_RAW_TABLE_SHA256_V1: [u8; 32] = [
    0x97, 0xbc, 0xc9, 0xf7, 0x93, 0xad, 0xb7, 0xf1, 0x3b, 0xd7, 0x0c, 0x89, 0xe9, 0x78, 0x8c, 0x8a,
    0xb6, 0x1b, 0xaf, 0x8c, 0x77, 0xe9, 0xf8, 0xcd, 0x80, 0x33, 0x5a, 0xd7, 0x67, 0xd7, 0x1a, 0xe2,
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
pub(crate) struct ExactNominalSrgb8CleanSetV1;

impl ExactNominalSrgb8CleanSetV1 {
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
