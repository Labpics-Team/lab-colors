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

## WCAG 2.2 для финальной sRGB8-пары — `wcag22.rs`, `wcag22/`, `srgb8.rs` (#284)

Это versioned terminal certificate соответствия объявленному success
criterion, а не замена LPC/APCA-shaped перцептивной цели solver-а. Клиент явно
передаёт criterion; core не выводит размер текста или семантику из имени токена.

| формула/инвариант | чем верифицирована | оракул |
|---|---|---|
| dated profile: IEC/WCAG EOTF split `0.04045`, веса `0.2126/0.7152/0.0722`, offset `0.05`, пороги `3.0` и `4.5` | immutable `wcag22-srgb8-v1.json`; независимая точная копия `NORMATIVE_PROFILE_V1` в `verify_wcag22_q55.py`; `wcag22_tests::*` | публикация (W3C WCAG 2.2 Recommendation 2024-12-12) |
| 768 outward Q55-вкладов (3 канала × 256 кодов) tight: ширина строки ≤ 1; все threshold terms overflow-safe | adaptive-precision Decimal с directed rounding и устойчивостью на successive precisions; exact-integer/fifth-power проверка каждой строки; verifier доказывает `180·(Q55+3)+7·Q55 < i64::MAX`, фиксирует headroom и отказ Q56 | независимая численная транскрипция + целочисленная проверка tightness/overflow |
| полный домен `256³ = 16 777 216` цветов имеет zero unresolved для обоих пороговых законов | `verify_wcag22_q55.py`: перечисление всех sRGB8-интервалов и monotone boundary scan; committed proof фиксирует минимальные pass/fail margins и witnesses; synthetic overlap обязан сделать verifier RED | полный конечный перебор + mutation oracle |
| production verdict использует только outward Q55 и целочисленные сравнения; kernel/parser/facade/terminal-evidence нельзя подменить отдельно | exact SHA узких production-листьев (`kernel.rs`, terminal evidence) и production-only parser-capsule в `srgb8.rs`, normalized facade SHA (исключены только три self-digest literal) + semantic guards; `anti_epsilon_witnesses_are_definite_fail`, parser panic/property tests | независимый verifier + внутренние boundary witnesses |
| proof связан с фактическим публичным путём, но не с посторонним текстом всего `lib.rs`/родительского `srgb8.rs` | source-binding schema v1: Cargo metadata фиксирует canonical lib target `src/lib.rs`; length-prefixed SHA-256 связывает этот факт, четыре crate-root route и exact parser body; независимый тест отвергает `[lib] path`, `cfg`/`path`/parser redirects и доказывает non-interference будущих `Srgb8`/root re-export; существующий compiled registry probe вызывает оба public entrypoint, 4.5 boundary и 4.5/3.0 criterion discriminator, evidence IDs и invalid transport | Cargo metadata + exact capsule oracle + внешний Rust consumer внутри verifier-а |
| право минтить terminal evidence связано с фактической typed WCAG registry-row | compiled Rust probe читает live row; Python канонизирует 10 mint-relevant полей через length-prefix/SHA-256; 10 field mutations + 2 hex/count transport mutations обязаны отказать | независимая site-local admission binding (в proof: 15 negative controls всего) |
| один verdict/evidence сохраняется через Core → FFI/WASM → JS/Swift/conformance | `wcag22_transport_*`, `wasm_parity`, `wcag22.test.mjs`, Swift conformance, committed pack 4 `wcag22.json`; release verifier повторно проверяет evidence-байты | дифференциальный cross-boundary oracle |

## Конечная WCAG 2.2 feasibility-компиляция — `wcag22_feasibility.rs` (#295)

Модуль канонизирует opaque client declarations и полностью перечисляет
зарегистрированный конечный домен. Он не ранжирует кандидаты, не выводит
применимость или размер текста из ID и не заменяет перцептивную цель solver-а.

| инвариант | чем верифицирован | оракул |
|---|---|---|
| все 256 нейтралей проверяются против каждой канонической applicable adjacency; граничные множества для 4.5:1 и 3:1 совпадают с independently recomputed fixture | `verify_wcag22_neutral_axis.py`; `production_vectors_are_bound_to_the_exact_independent_oracle_fixture`; full-matrix и boundary tests | независимая `Fraction`-арифметика с адаптивными точными границами корня пятой степени; production Q55/Rust evaluator не импортируются |
| перестановки и точные дубликаты не меняют канонические content IDs; изменение opaque ID меняет identity, но не физический partition | `verify_wcag22_feasibility_identity.py`; `exact_identity_preimages_match_the_independent_cross_language_fixture`; property/characterization tests канонизации | независимая Python-транскрипция exact byte grammar и SHA-256 + внутренние metamorphic tests |
| терминал появляется только после полного `W=256E`; packed storage равен `B=0` при `A=0`, иначе `B=32(E+1)`; partial terminal и silent fallback отсутствуют | fault-injection tests evaluator/storage/allocation/completeness; `check_wcag22_feasibility_benchmark.py` проверяет полный набор граничных shapes, exact counters и SHA-256 dependency cone | типизированные negative controls + raw native measurements; elapsed time не является acceptance threshold |

Benchmark artifact не доказывает total WebAssembly memory, serialized output
size или client latency: эти величины явно находятся вне его claim boundary.

## LPC (перцептивный контраст) — `lpc.rs`

| формула | чем верифицирована | оракул |
|---|---|---|
| `contrast_core` (APCA SAPC-8 `0.0.98G-4g`) | `golden_tests::contrast_core_matches_reference_on_grey_axis` (13 точек vs `apca-w3`) | эталонный софт (`apca-w3` v0.1.9) |
| Конечные точки (BoW ≈106.04, WoB ≈−107.88) | `lpc::tests::black_on_white_matches_reference`; `[NEW]` `reference_vectors::lpc_endpoints_match_apca_via_public_api` | публикация (APCA SAPC-8 endpoints) |
| `soft_clamp` / `soft_clamp_inv` | `lpc::tests::soft_clamp_boundaries_are_exact`, `..._matches_reference_bisection` | внутренняя тождественность |
| `y_hk_analytic` (обратный `grey_j`) | `lpc::tests::y_hk_analytic_matches_bisection_on_grid` | внутренняя тождественность |

## Appearance-граф — `crates/labcolors-core/src/appearance.rs`

Приватный компилятор/исполнитель физического компонента (#307). Модуль не несёт
собственной численной политики: единственная операция — SSOT-композитор
`alpha::composite_over_srgb8`; проверяется соответствие топологии и переносу
байтов, а не новая математика.

| формула/инвариант | чем верифицирована | оракул |
|---|---|---|
| source-over ребро графа ≡ `composite_over_srgb8` (весь домен байтов × α) | `appearance_graph_tests::graph_source_over_equals_the_independent_compositor_for_neutral_and_chromatic_inputs` (property) | дифференциальный (SSOT-композитор, сам верифицирован против reference-векторов `alpha.rs`) |
| replayable-сертификат: независимое повторение операции из полей сертификата побайтно равно записанному выходу | `appearance_graph_tests::source_over_certificate_replays_to_the_exact_recorded_bytes` (property) | внутренняя тождественность |
| канонизация: результат не зависит от порядка деклараций/значений typed handles | `appearance_graph_tests::compile_is_independent_of_declaration_order_for_the_same_handles`, `unrelated_opaque_handles_do_not_change_the_physics` | внутренняя тождественность |
| fail-closed: дубликаты/missing refs/циклы/дефекты bindings/α вне `[0,1]` — типизированные отказы | `appearance_graph_tests::compile_rejects_*`, `evaluate_rejects_*`, `graph_rejects_missing_occurrence_backdrop_and_cycles` | внутренняя тождественность |
| occurrence наблюдается против derived-поверхности, не страницы; identity-ребро не декоративно | `appearance_graph_tests::warning_occurrence_targets_the_rendered_surface_not_the_page` (+ trace-счётчики), `occurrence_source_follows_the_declared_identity_edge_not_the_composite_source` | внутренняя тождественность (witness) |
| production-миграция `PairLabel` байт-идентична замороженному legacy-пути (5 семей × 4 режима × 6 фонов + property + публичные отказы) | `pair_label_tests::migration_*` | дифференциальный (test-only legacy oracle) |

Мутационный скоуп: модуль включён в `.cargo/mutants.toml` (`examine_globs`).

## Численные решения — `numerics.rs`, `numerical_plan.rs` (#292)

Три уровня контракта разделены типами: package capability
(`NumericalCapabilityManifestV2` — единственная proof-capable projection
registry SSOT) ≠ compiled
invocation plan (`CompiledNumericalPlanV1`) ≠ атомарный результат
(`NumericalDecisionV1`: `Determinate`/`Compatibility`/`Indeterminate`).
Новой математики модуль не вводит — проверяется невозможность повышения
caller-created значений и legacy-исходов до доказательств.

| инвариант | чем верифицирован | оракул |
|---|---|---|
| registry непустой, ключи уникальны, Glow site покрыт обоими stable outcomes и registered compatibility release | `numerics::tests::migrated_registry_is_non_vacuous_unique_and_covers_glow_site` | внутренняя тождественность |
| capability manifest — каноническая projection registry: сортировка по UTF-8 `siteId`, coverage `migrated-sites-only-v1`, без выбранного mode | `numerics::tests::capability_manifest_is_canonical_registry_projection` | внутренняя тождественность |
| единственный public `numericalCapabilityManifest()` возвращает V2 с WCAG artifact/bound/proof IDs; одна декларация проецирует internal runtime и public capability без двух SSOT | `numerics::tests::unified_registry_projects_runtime_glow_and_proof_bound_wcag`, `projection::tests::capability_manifest_json_mirrors_proof_capable_core_ssot`, `packages/colors/test/capability-manifest.test.mjs` | regression pin + дифференциальный adapter/core |
| drift-checksum канонический и tamper-чувствителен: смена schema version / удаление row меняет FNV-1a-32 preimage | `numerics::tests::capability_checksum_is_canonical_and_tamper_sensitive`; независимые пересчёты: JS (`scripts/verify-package-release.mjs`) и Swift (`ConformanceTests.testCapabilityManifestChecksumRecomputes`) | внутренняя тождественность + два независимых re-implementation оракула |
| legacy-исход — атомарный `Compatibility` с registered release, не determinate evidence | `numerics::tests::legacy_result_is_compatibility_not_determinate_evidence` | тип-уровневая (взаимоисключающие варианты) + внутренняя тождественность |
| BitExact/bounded evidence минтится только registry-owned конструктором; внешний код не может ни собрать evidence, ни переупаковать genuine evidence другого site в новый terminal result | `numerics::tests::bit_exact_evidence_is_registry_owned_and_sealed`, `bit_exact_mint_is_refused_without_declared_capability` + три compile-fail doctests в шапке `numerics.rs` (удалённый classifier, приватная evidence-печать, cross-site reuse) | тип-уровневая (компилятор) |
| предметный Glow outcome также Core-owned: generic/WCAG evidence нельзя объявить `StableExactNoop`, а compatibility нельзя собрать вручную | variant-level sealing `GlowDecisionOutcomeV1` + compile-fail doctest в `glow.rs`; WASM-тесты получают оба outcome только через полный `resolve_named_set` path | тип-уровневая + boundary characterization |
| диагностический интервал проверяет только форму (конечность, порядок) и не изготовляет determinate evidence | `numerics::tests::diagnostic_interval_validates_shape_only` | внутренняя тождественность |
| invocation identity плана канонична: локальные ordinals внутри (node, site), перестановка деклараций не меняет ids/projection | `numerical_plan::tests::mixed_modes_coexist_and_ordinals_are_local`, `declaration_permutation_preserves_ids_and_canonical_projection` | внутренняя тождественность |
| план tamper-чувствителен: переименование node/site меняет identity, смена mode меняет checksum; незарегистрированный release — typed ошибка компиляции плана | `numerical_plan::tests::rename_changes_identity_and_mode_mutation_changes_checksum`, `unregistered_release_is_a_typed_compile_error` | внутренняя тождественность |
| conformance manifest публикует exact core projection (не рукописную копию) | `reference_runner::manifest_metadata_matches_core`, `labcolors_conformance::tests::manifest_numerical_registry_is_generated_from_core_ssot` | дифференциальный (закоммиченный артефакт против свежей генерации) |

## JS-дубликат — `packages/colors/effective-bg.js`

| формула | чем верифицирована | оракул |
|---|---|---|
| `srgbToLinear` / `linearToSrgb` (IEC 61966-2-1) | транзитивно через `parseOklch`/`oklabLerp` | внутренняя тождественность |
| `linearRgbToOklab` / `oklabToLinearRgb` (Ottosson) + `parseOklch` (oklch→sRGB байты) | `oklch-parse.test.mjs` (16 live-фикстур); `[NEW]` `reference-vectors.test.mjs` — байт-совпадение с ядром на ≥1000 сидированных строк (фикстура `test/data/oklch-core-vectors.txt`, эмитируется ядром `oklch_css_from_hex`, анти-дрейф — `reference_vectors::oklch_core_vectors_fixture_is_fresh`) | дифференциальный (ядро `labcolors-core` = оракул) |
| `parseCssColor` краевые (none/проценты/Chrome L∈0..1/out-of-gamut/H wrap) | `oklch-parse.test.mjs`; `[NEW]` `reference-vectors.test.mjs` edge-блок (семантика CSS Color 4) | публикация (CSS Color 4) |
