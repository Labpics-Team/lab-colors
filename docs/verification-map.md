# Карта верификации нижних слоёв

Каждая формула цветовых пространств и перцептивных метрик ядра `labcolors-core`
(и JS-дубликат в `packages/colors/effective-bg.js`) — против ВНЕШНЕГО
опубликованного эталона. Столбец «чем верифицирована» называет конкретный тест
и его оракул. `[NEW]` — добавлено веткой `test/reference-vectors` (внешние
опубликованные векторы); остальное существовало ранее.

Оракулы бывают трёх сортов:
- **публикация** — контрольные точки/векторы прямо из стандарта или статьи
  (IEC 61966-2-1, W3C WCAG 2.1 / CSS Color 4, Ottosson 2020, Li et al. 2017,
  Hellwig et al. 2022, APCA SAPC-8);
- **эталонный софт** — `colour-science` (Python), `apca-w3` (npm) — сам
  валидирован против стандартов CIE;
- **внутренняя тождественность** — round-trip / bit-identity / анти-дрейф
  (не внешний эталон, но ловит регрессию математики).

## sRGB — `crates/labcolors-core/src/spaces/srgb.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `srgb_gamma_inv` / `srgb_gamma` (EOTF/OETF IEC 61966-2-1 §6.4): стыки 0.04045 / 0.0031308, наклон 12.92, γ=2.4 | `[NEW]` `reference_vectors_deep::srgb_transfer_iec_control_points` + `..._join_is_continuous` + `..._css_color4_sample` | публикация (IEC 61966-2-1; CSS Color 4 sample decode(0.5)=0.214041) |
| `DECODE_8BIT` (точный 8-бит декод) | `srgb::tests::decode_table_matches_live_math`, `decode_reproduces_legacy_powf_path_for_every_byte` | внутренняя тождественность |
| `srgb_from_hex` / `hex_from_srgb` (квантизация 8-бит) | `srgb::tests::hex_round_trip_is_identity_for_all_grey_codes`; `oklch::tests::round_trip_is_byte_exact_on_lattice` | внутренняя тождественность |
| `SRGB_TO_XYZ_D65` / `XYZ_D65_TO_SRGB` (матрицы CSS Color 4) | `[NEW]` `reference_vectors_deep::srgb_xyz_matrices_are_mutual_inverses` + `..._white_maps_to_d65` | публикация (W3C CSS Color 4 / IEC 61966-2-1) |
| `D65_WHITE` (из хроматичности 0.3127/0.3290) | `[NEW]` `reference_vectors_deep::d65_white_derives_from_chromaticity` | публикация (IEC 61966-2-1 / CSS Color 4) |

## Oklab / OKLCH — `spaces/oklab.rs`, `spaces/oklch.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `SRGB_TO_LMS` … `LMS_TO_SRGB` (матрицы Ottosson 2021-01-25) → `srgb_linear_to_oklab` | `[NEW]` `reference_vectors_deep::oklab_matches_ottosson_xyz_table` (4 строки XYZ→Lab из поста Ottosson) | публикация (Ottosson 2020, таблица XYZ→Oklab) |
| Белая точка D65 → `L=1, a=0, b=0` | `oklab::tests::white_gives_l1_a0_b0`; `[NEW]` `reference_vectors::oklab_white_is_l1_c0` (публичный `oklch_from_hex`) | публикация (Ottosson design-constraint / XYZ-table строка 1) |
| `oklab_to_srgb_linear` (обратный путь) | `oklab::tests::roundtrip_five_colors` | внутренняя тождественность |
| `oklab_hue` / полярная форма OKLCH | `oklab::tests::hue_returns_degrees_0_360`; `[NEW]` `reference_vectors::oklch_primary_hues` (красный≈29°, зелёный≈142°, синий≈264°) | публикация (Ottosson/CSS Color 4 канонические углы) |
| `oklch_css_from_hex` (эмиссия), байт-точность | `oklch::tests::round_trip_is_byte_exact_on_lattice/_greys`; PR #149/#150 гетеро-оракулы | внутренняя тождественность |

## CAT16 / CIECAM16 — `spaces/cat16.rs`, `spaces/cam16.rs`, `spaces/vc.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `XYZ_TO_CONE` / `CONE_TO_XYZ` (CAT16, Li et al. 2017) | `[NEW]` `reference_vectors_deep::cat16_printed_inverse_residual` (‖M·M⁻¹−I‖≈5.4e-9); транзитивно golden CAM16 | публикация (Li et al. 2017, печатная обратная матрица) |
| `adapt` / `unadapt` (пост-адаптационное сжатие, Li et al. 2017) | `[NEW]` `reference_vectors_deep::cam16_adapt_matches_published_closed_form` + `..._adapt_unadapt_round_trip` | публикация (Li et al. 2017 / CIE 248:2022) |
| `forward` (CIECAM16 `XYZ→J,M,h`) | `golden_tests::cam16_matches_colour_science_{average,dim}_surround` (24 вектора) | эталонный софт (colour-science) |
| `ucs_j`/`ucs_m` и обратные (CAM16-UCS, Li et al. 2017) | `cam16::tests::ucs_rescale_round_trips`; `[NEW]` `reference_vectors_deep::cam16_ucs_constants` | публикация + внутренняя тождественность |
| `ViewingConditions::build` (`F_L, n, z, N_bb, A_w, D, RGB_D`) | `[NEW]` `reference_vectors::cam16_viewing_conditions_derivation` (независимая транскрипция CIE 248:2022 vs публичные поля) | публикация (Li et al. 2017 / CIE 248:2022) |

## Helmholtz–Kohlrausch — `lpc.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `hk_coeff` — f(h) = −0.160cos h + 0.132cos 2h − 0.405sin h + 0.080sin 2h + 0.792 (**Hellwig et al. 2022**, DOI 10.1002/col.22793) | `[NEW]` `reference_vectors_deep::hk_coeff_matches_hellwig2022_published` (коэффициенты дословно из статьи/`colour-science`) | публикация (Hellwig 2022) |
| `J_HK = J + f(h)·C^0.587` | `lpc::tests::j_hk_matches_hellwig_reference` (12 якорей vs colour-science) | эталонный софт (colour-science 0.4.7) |
| Знак H-K (насыщ. синий поднят) | `lpc::tests::blue_on_white_below_achromatic`; `[NEW]` `reference_vectors::hk_lifts_saturated_blue_via_public_lpc` | публикация (эффект H-K) |

> **Находка:** используется формула **Hellwig, Stolitzka & Fairchild (2022)**,
> НЕ Nayatani/VAC и НЕ Fairchild-1998. Подтверждено дословным сравнением
> коэффициентов с `colour-science` `hue_angle_dependency_Hellwig2022`.

## WCAG 2.1 — `wcag.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `linearise` (порог 0.03928, /12.92, γ 2.4) | `[NEW]` `reference_vectors_deep::wcag_linearise_threshold_is_original_03928` | публикация (W3C WCAG 2.1 §1.4.3, оригинальная версия 2018) |
| `relative_luminance` (0.2126/0.7152/0.0722) | `[NEW]` `reference_vectors::wcag_luminance_coefficients_isolated` (primary-on-black изолирует каждый коэффициент) | публикация (W3C WCAG 2.1) |
| `contrast_ratio` (L↑+0.05)/(L↓+0.05) | `wcag::tests::black_on_white_is_twentyone_to_one`, `grey_boundary_matches_published_value` (#767676≈4.54); `[NEW]` `reference_vectors::wcag_published_ratios_via_public_api` | публикация (W3C WCAG 2.1) |

> **Находка (зафиксирована в тесте):** порог `0.03928` — ОРИГИНАЛЬНАЯ версия
> WCAG 2.1 (2018). Erratum W3C от 2022-02-22 (PR #1780, вошёл в Рекомендацию
> 05.2025) поднял порог до `0.04045` (= стык IEC EOTF) — то есть текущий
> нормативный текст говорит `0.04045`, а `0.03928` устарел. lab-colors
> сознательно держит `0.03928`. Ни один 8-битный код не попадает в интервал
> (0.03928, 0.04045) — 10/255 ≈ 0.039216 ниже обоих, 11/255 ≈ 0.043137 выше — то
> есть для КАЖДОГО квантованного цвета обе версии выбирают одну ветвь и
> линеаризуют идентично; расхождение только на суб-квантовых величинах.

## LPC (перцептивный контраст) — `lpc.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `contrast_core` (APCA SAPC-8 `0.0.98G-4g`) | `golden_tests::contrast_core_matches_reference_on_grey_axis` (13 точек vs `apca-w3`) | эталонный софт (`apca-w3` v0.1.9) |
| Конечные точки (BoW ≈106.04, WoB ≈−107.88) | `lpc::tests::black_on_white_matches_reference`; `[NEW]` `reference_vectors::lpc_endpoints_match_apca_via_public_api` | публикация (APCA SAPC-8 endpoints) |
| `soft_clamp` / `soft_clamp_inv` | `lpc::tests::soft_clamp_boundaries_are_exact`, `..._matches_reference_bisection` | внутренняя тождественность |
| `y_hk_analytic` (обратный `grey_j`) | `lpc::tests::y_hk_analytic_matches_bisection_on_grid` | внутренняя тождественность |

## Численные решения — `numerics.rs` (#292)

Три уровня контракта разделены типами: package capability (registry-строка) ≠
compiled invocation plan (`CompiledNumericalPlanV1`) ≠ result evidence
(запечатанный `SoundIntervalEvidenceV1`). Новой математики модуль не вводит —
проверяется невозможность повышения caller-created значений до доказательств.

| инвариант | чем верифицирован | оракул |
|---|---|---|
| план компилируется fail-closed из machine-readable строки: stable = [exact, refuse], legacy требует объявленного профиля | `numerics::tests::stable_plan_for_glow_site_admits_only_exact_check_and_refusal`, `legacy_plan_requires_a_declared_compatibility_profile` | внутренняя тождественность |
| caller-created интервал не достигает determinate-гарантии: классификатор принимает только запечатанное свидетельство (конструктор приватен) | сигнатура `classify_at_least_v1` + `interval_evidence_carries_its_provenance_into_the_certificate` | тип-уровневая (компилятор) + внутренняя тождественность |
| граница `>=` на интервале: Meets/Below/Overlap без tie-break, exact-касание цели детерминировано | `numerics::tests::interval_overlap_never_becomes_a_tie_break` | внутренняя тождественность |
| production-ветви glow исполняются строго по плану (порядок методов — из registry, не из рукописного match) | glow-набор (32 теста) + пробная мутация плана (5 падений glow, 1 numerics) | дифференциальный (замороженное glow-поведение) |

## JS-дубликат — `packages/colors/effective-bg.js`

| формула | чем верифицирована | оракул |
|---|---|---|
| `srgbToLinear` / `linearToSrgb` (IEC 61966-2-1) | транзитивно через `parseOklch`/`oklabLerp` | внутренняя тождественность |
| `linearRgbToOklab` / `oklabToLinearRgb` (Ottosson) + `parseOklch` (oklch→sRGB байты) | `oklch-parse.test.mjs` (16 live-фикстур); `[NEW]` `reference-vectors.test.mjs` — байт-совпадение с ядром на ≥1000 сидированных строк (фикстура `test/data/oklch-core-vectors.txt`, эмитируется ядром `oklch_css_from_hex`, анти-дрейф — `reference_vectors::oklch_core_vectors_fixture_is_fresh`) | дифференциальный (ядро `labcolors-core` = оракул) |
| `parseCssColor` краевые (none/проценты/Chrome L∈0..1/out-of-gamut/H wrap) | `oklch-parse.test.mjs`; `[NEW]` `reference-vectors.test.mjs` edge-блок (семантика CSS Color 4) | публикация (CSS Color 4) |
