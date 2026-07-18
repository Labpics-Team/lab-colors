# ADR-0003. H-K не входит в Ys candidate score

Статус: **реализовано**.

## Решение

Замороженная SAPC-shaped кривая получает display-referred относительную
люминансу `Ys`. Поправка Гельмгольца–Кольрауша (H-K) остаётся appearance-
коррелятом яркости и не подставляется вместо `Ys` в этот численный путь.

Это разделение доменов, а не выбор между «цветом» и «серым»:

- `solve` и runtime recheck вычисляют знаковую кандидатную оценку по `Ys` уже
  квантованных display-цветов;
- эта оценка остаётся внутренней координатой solver/recheck, а не публичным
  LPC/readability verdict;
- H-K сохраняется во внутренней appearance-математике и её характеризации;
- brightness-matching семейных цветов сохраняет H-K;
- внутренний `pair_side` использует `Ys`-кривую и H-K appearance-координату в
  разных членах своей переходной эвристики. Ни одна из них не выдаётся за
  универсальную модель разборчивости.

## Почему

Перенесённые в `contrast_core` константы SAPC-8 определены над экранной
люминансой. H-K, напротив, оценивает рост воспринимаемой яркости насыщенного
стимула при той же фотометрической люминансе. Подстановка H-K-яркости меняет
домен входа опубликованной формулы и смешивает разные величины.

Разделение согласуется с двумя независимыми границами применимости:

- мелкая пространственная структура значительно лучше разрешается
  люминансным, чем хроматическим каналом (Mullen, 1985);
- H-K-модель описывает perceived brightness, а не читаемость текста
  (Hellwig, Stolitzka, Fairchild, 2022).

Эти источники обосновывают разделение доменов, но не валидируют текущую кривую
как LPC или модель читаемости. Размер, начертание, пространственная частота,
адаптация, поле фона и задача наблюдателя принадлежат отдельному versioned
evaluator/applicability profile.

## Исполняемая граница

Production-путь закреплён кодом и тестами:

- `solve_lpc_lightness`, `match_lightness_ys` и `finish` решают и повторно
  измеряют цель в `Ys`;
- `semantic::measure_contrast` и `recheck_against*` используют ту же
  display-люминансу;
- низкоуровневый вход кривой принимает только уже проверенные finite
  display-люминансы и не является публичной parser boundary;
- H-K-математика остаётся доступна только там, где контракт явно относится к
  appearance/brightness.

Публичного scalar-LPC API до versioned evaluator-профиля нет. Поле `lc`, которое
пока проходит через resolver и wire, — знаковая кандидатная оценка по `Ys` из
замороженной SAPC-shaped кривой, а не самостоятельная модель читаемости.

Численные пороги и коэффициенты не определяются этим ADR. Их provenance и
executable gates принадлежат соответствующим формулам и inventory.

## Следствия

- Ys candidate score не зависит от viewing-condition компенсации яркости;
- H-K не удаляется и не маскируется под APCA/LPC;
- одинаковая display-пара получает один candidate score в solver и runtime recheck;
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
