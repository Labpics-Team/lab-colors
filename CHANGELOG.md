# Changelog

Все существенные изменения Lab Colors фиксируются в этом файле. Версии npm и
Rust различаются, потому что это разные delivery surfaces одного контракта.

## [@labpics/colors 0.10.0 / Rust 0.2.0] - 2026-07-11

Breaking release относительно `@labpics/colors` 0.9.1 / Rust 0.1.0. Пошаговый
переход и rollback: [exact alpha / typed Glow](docs/migrations/exact-alpha-glow.md).

### Breaking

- Каждый client-owned Glow-рецепт требует explicit `decision_profile`; default
  и silent legacy fallback удалены. Профиль входит в config fingerprint.
- `GlowRole` стал union из determinate `kind: "glow"` и typed terminal
  `kind: "glow-indeterminate"`. Indeterminate не эмитит halo/core/alpha CSS vars.
- Единый `referenceProfile` из pre-release API заменён раздельными
  `compositeProfile` / `compositeGuarantee`, `diagnosticProfile` и
  `decisionProfile` / `decisionGuarantee`.
- `solve_screen_alpha_for_dj` принимает `GlowDecisionProfileV1` и возвращает
  `NumericalDecisionV1<GlowSolve>`; поля `GlowSolve` доступны через getters.
- `resolve_alpha_analog_hex` возвращает `Result<(String, f64), String>` вместо
  вложенного `Option`. Для валидного домена sRGB8-ответ тотален; недоменная
  alpha возвращает `Err`, а не клампится.
- Публичные continuous/point compositor-границы возвращают `Result` и одинаково
  отвергают нечисловой или внедиапазонный ввод в debug/release.

### Added

- Exact encoded-sRGB8 source-over и screen profiles с bit-exact composite
  certificate, binary64 identity alpha и канонической `alphaCss`.
- Machine-readable registry branch-sensitive numerical sites и typed
  `Determinate` / `Indeterminate` с причиной и sound bounds либо честным
  `bounds: unavailable`.
- Determinate Glow сообщает отдельные halo/core point-композиты и diagnostic
  `|ΔJ′|`, target status, constraint layer и классы гарантий.
- Conformance pack 2.0.0 с alpha half-tie-вектором.
- Публикуемый `labcolors-core` теперь несёт package-local README; CI проверяет
  реальный `.crate`, распаковывает его и исполняет doctest вне workspace tree.
- `@labpics/colors/build-metadata.json` экспортирует machine-readable связь
  npm/core versions, source SHA, conformance hashes и точных WASM bytes/hash;
  release verifier повторно сверяет её после чистой установки tarball.

### Fixed

- Source-over half-tie считается в byte-reference порядке: `#C0B2FA @ 0.122`
  над `#000000` даёт `#17161F`, без потери соседнего LSB при нормализации.
- Glow alpha больше не округляется независимо от выбранного sRGB8-state:
  `alphaCss` round-trip восстанавливает ту же binary64 alpha и тот же композит.
- Нетривиальный stable CAM16 target/max-site без sound error bound больше не
  получает правдоподобный platform-selected verdict: результат typed
  `Indeterminate`.
- Exact stable no-op больше не маркируется как выполненная CAM16-диагностика:
  `diagnosticProfile` честно равен `null` на всех delivery boundaries.
- `BackdropBox` с reversed, non-finite или внедиапазонными bounds отклоняется
  как недоменный public input (`None`) без swap, clamp или debug-паники.

### Compatibility evidence

- Explicit `legacy-platform-dependent-v1` сохраняет прежний CAM16/libm path и
  CSS-эмиссию, но не маркируется stable numerical guarantee.
- В закреплённом Lab UI corpus изменились ровно 24 Glow alpha-листа
  `resolveVars` (4 ключа на 6 записях); 1376 пар `(lc, wcag)` recheck остались
  байт-идентичны. Это ограниченное свидетельство для pinned corpus, не
  универсальная гарантия произвольного клиента.

### Known limits

- Для нетривиального CAM16 target/max-выбора sound cross-runtime bound пока не
  установлен; `stable-v1` намеренно возвращает `Indeterminate`.
- Exact point-композит не является сертификатом browser color-management,
  дисплея/HDR или пространственного blur/overlap-эффекта.
