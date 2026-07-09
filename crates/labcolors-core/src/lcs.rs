use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_to_xyz, xyz_to_srgb};
use crate::spaces::{cam16, cat16, oklab, vc::ViewingConditions};

/// Все поля hue (`h_ok`, `h_cam`) хранятся в **градусах** `[0, 360)`.
/// Радианы создаются только в месте тригонометрического вызова, чтобы единицы
/// нельзя было спутать в состоянии цвета.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LcsColor {
    pub jp: f64,
    pub h_ok: f64,
    /// Внутренняя репараметризация красочности CAM16-UCS `M′`:
    /// `s = M′ / (J′ + 1)`. The `+ 1` is a regulariser against division by zero
    /// при `J′ → 0`; преобразование обратимо — `LcsColor::mp` восстанавливает `M′` как
    /// `s · (J′ + 1)`. This is NOT the CAM16 saturation correlate.
    pub s: f64,
    h_cam: f64,
}

impl LcsColor {
    /// Декодирует hex в стандартных условиях sRGB со средним окружением.
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        Self::from_hex_with_vc(hex, &ViewingConditions::srgb())
    }

    /// Декодирует hex при заданных условиях просмотра.
    ///
    /// Результат зависит от условий просмотра: тёмная тема, например, должна
    /// передать [`ViewingConditions::dim_surround`], а не переиспользовать
    /// координаты светлого окружения.
    pub fn from_hex_with_vc(hex: &str, vc: &ViewingConditions) -> Result<Self, String> {
        let rgb = srgb_from_hex(hex)?;
        let xyz = srgb_to_xyz(rgb);
        let h_ok = oklab::oklab_hue(rgb);
        Ok(Self::from_xyz_with_hok(xyz, h_ok, vc))
    }

    /// Кодирует в hex в стандартных условиях просмотра sRGB.
    pub fn to_hex(&self) -> String {
        self.to_hex_with_vc(&ViewingConditions::srgb())
    }

    /// Кодирует в hex при заданных условиях просмотра.
    ///
    /// Условия должны совпадать с условиями построения цвета, иначе round-trip
    /// закономерно представит другой воспринимаемый стимул.
    pub fn to_hex_with_vc(&self, vc: &ViewingConditions) -> String {
        let xyz = self.to_xyz(vc);
        let rgb = xyz_to_srgb(xyz);
        hex_from_srgb(rgb)
    }

    /// Конструктор для уже проверенных координат кривых и решателя.
    /// Пользовательский ввод сюда не попадает, поэтому повторная валидация не нужна.
    pub(crate) fn new(jp: f64, h_ok: f64, s: f64, h_cam: f64) -> Self {
        Self { jp, h_ok, s, h_cam }
    }

    /// Декартовы координаты CAM16-UCS `[J′, a′, b′]`.
    ///
    /// Евклидова метрика CAM16-UCS определена через
    /// `a′ = M′ cos(h_cam)` and `b′ = M′ sin(h_cam)`.  In particular, the
    /// Направление обязано быть CAM16 hue, сопряжённым с величиной CAM16-UCS.
    /// Сочетание `M′` с Oklab hue не принадлежит ни одной из моделей и не имеет
    /// корректной интерпретации евклидова расстояния.
    pub fn ucs_cartesian(&self) -> [f64; 3] {
        let h = self.h_cam.to_radians();
        let mp = self.mp();
        [self.jp, mp * h.cos(), mp * h.sin()]
    }

    /// Стандартная евклидова цветовая разность CAM16-UCS до `other`.
    pub fn delta_e_ucs(&self, other: &Self) -> f64 {
        let a = self.ucs_cartesian();
        let b = other.ucs_cartesian();
        let dj = a[0] - b[0];
        let da = a[1] - b[1];
        let db = a[2] - b[2];
        dj.hypot(da).hypot(db)
    }

    /// Красочность CAM16-UCS `M'`, обратимо восстановленная из `s`.
    pub(crate) fn mp(&self) -> f64 {
        self.s * (self.jp + 1.0)
    }

    /// Красочность CAM16-UCS `M'` допустимого **линейного** sRGB без hex round-trip.
    ///
    /// `M'` не зависит от сопровождающего Oklab hue, поэтому путь ограничен
    /// `rgb → XYZ → CAM16 → M'`. Это безаллокационный эквивалент hex-round-trip
    /// для уже квантованного `rgb`.
    pub(crate) fn mp_of_linear_srgb(rgb: [f64; 3], vc: &ViewingConditions) -> f64 {
        let xyz = srgb_to_xyz(rgb);
        // `h_ok` не влияет на M′, поэтому ноль исключает лишний расчёт Oklab hue.
        Self::from_xyz_with_hok(xyz, 0.0, vc).mp()
    }

    /// CAM16 hue в градусах. Приватность поля не даёт случайно смешать его с
    /// `h_ok`: первый обслуживает CAM16, второй — геометрию Oklab.
    pub(crate) fn h_cam(&self) -> f64 {
        self.h_cam
    }

    /// Строит согласованное LCS-значение из полярных координат CAM16-UCS.
    ///
    /// `h_ok` выводится из результата обратного CAM16. Так вызывающий код не
    /// сможет независимо интерполировать два hue-трека и создать состояние, где
    /// публичный геометрический hue описывает не тот цвет, что [`Self::to_xyz`].
    pub(crate) fn from_ucs_polar(jp: f64, mp: f64, h_cam: f64, vc: &ViewingConditions) -> Self {
        let jp = jp.max(0.0);
        let mp = mp.max(0.0);
        let h_cam = h_cam.rem_euclid(360.0);
        let s = if jp + 1.0 > 0.0 { mp / (jp + 1.0) } else { 0.0 };
        let mut out = Self {
            jp,
            h_ok: 0.0,
            s,
            h_cam,
        };
        if mp > 0.0 && jp > 0.0 {
            let rgb = xyz_to_srgb(out.to_xyz(vc));
            out.h_ok = oklab::oklab_hue(rgb);
        }
        out
    }

    /// Линейные координаты sRGB без clipping, представленные этим значением.
    pub(crate) fn to_linear_srgb(self, vc: &ViewingConditions) -> [f64; 3] {
        xyz_to_srgb(self.to_xyz(vc))
    }

    pub(crate) fn from_xyz_with_hok(xyz: [f64; 3], h_ok: f64, vc: &ViewingConditions) -> Self {
        // Единый прямой проход CIECAM16 не даёт двум потребителям разойтись в
        // формулах; LCS добавляет только UCS-масштабирование.
        let (j, m, h) = cam16::forward(xyz, vc);
        Self::from_cam16(j, m, h, h_ok)
    }

    /// Строит цвет из готовых коррелятов CIECAM16 `(J, M, h_cam)` и Oklab hue.
    /// Повторный forward не выполняется, чтобы решатель переиспользовал один и
    /// тот же физический расчёт без численного расхождения.
    pub(crate) fn from_cam16(j: f64, m: f64, h_cam: f64, h_ok: f64) -> Self {
        // Масштабирование CAM16-UCS (Li et al. 2017, DOI 10.1002/col.22131)
        // вынесено в общие функции, чтобы прямой и обратный пути не расходились.
        let jp = cam16::ucs_j(j);
        let mp = cam16::ucs_m(m);
        let s = mp / (jp + 1.0);

        Self { jp, h_ok, s, h_cam }
    }

    pub(crate) fn to_xyz(self, vc: &ViewingConditions) -> [f64; 3] {
        // Обратное масштабирование берётся из того же SSOT в `cam16`.
        let j = cam16::ucs_j_inv(self.jp);
        let m = cam16::ucs_m_inv(self.mp());

        // В физическом чёрном J=M=0, а общая обратная формула содержит
        // `M / sqrt(J)`. Аналитическая ветвь исключает искусственный 0/0 и
        // последующий NaN-clipping; M>0 при J=0 лежал бы вне цветового тела.
        if j <= 0.0 {
            debug_assert!(
                m == 0.0,
                "positive CAM16 colourfulness at J=0 is unrealizable"
            );
            return [0.0, 0.0, 0.0];
        }
        let hr = self.h_cam.to_radians();
        // `hr.cos()` / `hr.sin()` ниже вычислялись дважды каждый; считаем один раз
        // и переиспользуем — байт-идентичный CSE (тот же аргумент, тот же
        // libm-вызов). `e_hue` берёт другой аргумент (`hr + 2.0`) и не трогается.
        let cos_hr = hr.cos();
        let sin_hr = hr.sin();

        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        // `vc.t_inner` == `(1.64 - 0.29^n)^0.73`, `vc.fl_pow_025` == `fl^0.25`: те
        // же пер-VC константы, что и в прямом ходе, вынесенные из пер-цветовой
        // инверсии. Порядок умножения сохранён → байт-идентично прежнему инлайну
        // `t_inner * vc.fl.powf(0.25)`.
        let t = (m / ((j / 100.0).sqrt() * vc.t_inner * vc.fl_pow_025)).powf(1.0 / 0.9);

        let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.nbb;
        let p2 = (vc.aw * (j / 100.0).powf(1.0 / (vc.c * vc.z))) / vc.nbb;
        let gamma = 23.0 * (p2 + 0.305) * t / (23.0 * p1 + 11.0 * t * cos_hr + 108.0 * t * sin_hr);

        let a = gamma * cos_hr;
        let b = gamma * sin_hr;

        let r_a = (460.0 * p2 + 451.0 * a + 288.0 * b) / 1403.0;
        let g_a = (460.0 * p2 - 891.0 * a - 261.0 * b) / 1403.0;
        let b_a = (460.0 * p2 - 220.0 * a - 6300.0 * b) / 1403.0;

        let r_c = cam16::unadapt(r_a, vc.fl);
        let g_c = cam16::unadapt(g_a, vc.fl);
        let b_c = cam16::unadapt(b_a, vc.fl);

        let lms = [r_c / vc.rgb_d[0], g_c / vc.rgb_d[1], b_c / vc.rgb_d[2]];
        let xyz = cat16::cone_to_xyz(lms);

        [xyz[0] / 100.0, xyz[1] / 100.0, xyz[2] / 100.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_neutral_base() {
        let original = "#787880";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_white() {
        let original = "#FFFFFF";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn roundtrip_dark() {
        let original = "#101012";
        let lcs = LcsColor::from_hex(original).unwrap();
        let back = lcs.to_hex();
        assert!(
            back.eq_ignore_ascii_case(original),
            "roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn from_hex_rejects_short_string() {
        assert!(LcsColor::from_hex("#fff").is_err());
    }

    #[test]
    fn h_ok_stable_across_roundtrip() {
        let original = "#787880";
        let lcs1 = LcsColor::from_hex(original).unwrap();
        let back = lcs1.to_hex();
        let lcs2 = LcsColor::from_hex(&back).unwrap();
        assert!(
            (lcs1.h_ok - lcs2.h_ok).abs() < 1e-6,
            "h_ok drift: {} vs {}",
            lcs1.h_ok,
            lcs2.h_ok
        );
    }

    #[test]
    fn roundtrip_dim_surround_midgrey() {
        let vc = ViewingConditions::dim_surround();
        let original = "#787880";
        let lcs = LcsColor::from_hex_with_vc(original, &vc).unwrap();
        let back = lcs.to_hex_with_vc(&vc);
        assert!(
            back.eq_ignore_ascii_case(original),
            "dim roundtrip drift: expected {original}, got {back}"
        );
    }

    #[test]
    fn dim_jp_differs_from_srgb() {
        let vc = ViewingConditions::dim_surround();
        let avg = LcsColor::from_hex("#787880").unwrap();
        let dim = LcsColor::from_hex_with_vc("#787880", &vc).unwrap();
        assert!(
            (avg.jp - dim.jp).abs() > 0.1,
            "same stimulus should produce different J' across VCs: avg={} dim={}",
            avg.jp,
            dim.jp,
        );
    }

    #[test]
    fn wrong_vc_roundtrip_drifts() {
        // Construct with dim VC, convert with srgb VC → should drift
        let dim_vc = ViewingConditions::dim_surround();
        let lcs = LcsColor::from_hex_with_vc("#787880", &dim_vc).unwrap();
        let wrong_hex = lcs.to_hex(); // uses srgb VC — mismatch!
        // The hex will still be valid sRGB, just not matching the original
        assert!(
            !wrong_hex.eq_ignore_ascii_case("#787880"),
            "VC mismatch should cause drift, got {}",
            wrong_hex,
        );
    }

    #[test]
    fn h_cam_stored_in_degrees() {
        // CAM16 hue of sRGB red is tens of degrees; a value below 2π would
        // mean radians leaked into storage.
        let red = LcsColor::from_hex("#FF0000").expect("#FF0000 is a valid hex colour");
        let h = red.h_cam();
        assert!((0.0..360.0).contains(&h), "h_cam out of range: {}", h);
        assert!(
            h > 7.0,
            "red CAM16 hue should be tens of degrees, got {} — radians leak?",
            h
        );
    }
}
