//! Детерминированный ГПСЧ (SplitMix64) — единственный источник случайности харнесса.
//!
//! Одна и та же посадочная зерно-величина (seed) даёт байт-идентичный поток, поэтому
//! манифест сессии, bootstrap-ресэмплы и синтетический наблюдатель полностью
//! воспроизводимы. Алгоритм — SplitMix64 (Steele, Lea, Flood, 2014): один `u64`
//! состояния, финализатор из констант, равномерное качество на наших объёмах
//! (десятки тысяч тяг). Криптостойкость не требуется и не заявляется.

/// SplitMix64 — потоковый ГПСЧ с 64-битным состоянием.
#[derive(Debug, Clone)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Создаёт поток из зерна. Любое `u64` допустимо (включая ноль).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Следующее равномерное 64-битное значение.
    pub fn next_u64(&mut self) -> u64 {
        // Константы SplitMix64: инкремент — золотое сечение (0x9E3779B97F4A7C15),
        // два xorshift-умножения финализатора перемешивают биты.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Равномерное `f64` в полуинтервале `[0, 1)`.
    ///
    /// Берёт старшие 53 бита (мантисса `f64`) и делит на `2^53`, что даёт
    /// равномерную сетку без смещения на границах.
    pub fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11; // 53 старших бита
        bits as f64 / ((1u64 << 53) as f64)
    }

    /// Равномерное целое в `[0, n)` без модуло-смещения (Lemire's rejection).
    ///
    /// Возвращает `0`, если `n == 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        // Отбрасываем «хвост», который бы исказил равномерность (rejection).
        let threshold = (n.wrapping_neg()) % n; // == 2^64 mod n
        loop {
            let r = self.next_u64();
            if r >= threshold {
                return r % n;
            }
        }
    }

    /// Перемешивание на месте (Fisher-Yates), несмещённое.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        let len = slice.len();
        for i in (1..len).rev() {
            let j = self.below((i + 1) as u64) as usize;
            slice.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let mut a = SplitMix64::new(12345);
        let mut b = SplitMix64::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seed_diverges() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(2);
        // Крайне маловероятно совпасть на первой тяге при разных зёрнах.
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn f64_in_unit_interval() {
        let mut r = SplitMix64::new(99);
        for _ in 0..100_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x), "x={x} вне [0,1)");
        }
    }

    #[test]
    fn below_is_in_range_and_covers() {
        let mut r = SplitMix64::new(7);
        let mut seen = [false; 6];
        for _ in 0..10_000 {
            let v = r.below(6) as usize;
            assert!(v < 6);
            seen[v] = true;
        }
        assert!(seen.iter().all(|&s| s), "below(6) не покрыл все грани");
    }

    #[test]
    fn shuffle_is_permutation() {
        let mut r = SplitMix64::new(555);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn below_zero_is_zero() {
        let mut r = SplitMix64::new(1);
        assert_eq!(r.below(0), 0);
    }
}
