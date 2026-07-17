# ADR-0003. H-K не входит в ось читаемости

Статус: **реализовано**.

## Решение

Ось читаемости Lab Colors применяет контрастную кривую к display-referred
относительной люминансе `Ys`. Поправка Гельмгольца–Кольрауша (H-K) остаётся
координатой воспринимаемой яркости и не подставляется вместо `Ys` при проверке
текстового или UI-контраста.

Это разделение доменов, а не выбор между «цветом» и «серым»:

- `solve` и runtime recheck измеряют readability-контраст по `Ys` уже
  квантованных display-цветов;
- `lpc_readability_ys` является низкоуровневым reference-входом той же формулы;
- `lpc`/`lpc_lcs` сохраняют H-K как appearance-метрику;
- brightness-matching сентимента сохраняет H-K;
- `pair_side` явно считает белую сторону в `Ys`, а appearance-оценку чернильной
  стороны отдельно. Ни одна из этих координат не выдаётся за универсальную
  модель разборчивости.

## Почему

Перенесённые в `contrast_core` константы SAPC-8 определены над экранной
люминансой. H-K, напротив, оценивает рост воспринимаемой яркости насыщенного
стимула при той же фотометрической люминансе. Подстановка H-K-яркости в
readability-кривую смешивает разные величины и может изменить выбранную
полярность на насыщенном фоне.

Разделение согласуется с двумя независимыми границами применимости:

- мелкая пространственная структура значительно лучше разрешается
  люминансным, чем хроматическим каналом (Mullen, 1985);
- H-K-модель описывает perceived brightness, а не читаемость текста
  (Hellwig, Stolitzka, Fairchild, 2022).

Это не доказывает универсальную читаемость. Размер, начертание, пространственная
частота, адаптация, поле фона и задача наблюдателя принадлежат более высокому
профилю LPC/applicability и не выводятся из одной пары цветов.

## Исполняемая граница

Production-путь закреплён кодом и тестами:

- `solve_lpc_lightness`, `match_lightness_ys` и `finish` решают и повторно
  измеряют цель в `Ys`;
- `semantic::measure_contrast` и `recheck_against*` используют ту же
  display-люминансу;
- `lpc_readability_ys` принимает уже проверенные finite sRGB-каналы в `[0, 1]`
  и не является parser boundary;
- H-K-математика остаётся доступна только там, где контракт явно относится к
  appearance/brightness.

Численные пороги и коэффициенты не определяются этим ADR. Их provenance и
executable gates принадлежат соответствующим формулам и inventory.

## Следствия

- readability-координата не зависит от viewing-condition компенсации яркости;
- H-K не удаляется и не маскируется под APCA/LPC;
- одинаковая display-пара получает одну readability-оценку в solver и runtime
  recheck;
- изменение этой границы требует отдельного versioned evidence profile и
  дифференциальной проверки всего пути resolve → emit → recheck.

## Источники

- Mullen K. T. (1985), *The contrast sensitivity of human colour vision to
  red-green and blue-yellow chromatic gratings*, Journal of Physiology 359,
  381–400.
- Hellwig L., Stolitzka D., Fairchild M. D. (2022), *Extending CIECAM02 and
  CAM16 for the Helmholtz–Kohlrausch effect*, Color Research & Application
  47(5), 1096–1104, DOI `10.1002/col.22793`.
- Myndex Research, `apca-w3`, reference implementation of the SAPC/APCA
  display-luminance curve used as the source of the `contrast_core` constants.
