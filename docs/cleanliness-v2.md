# ColorQuality: научный контракт экранного анализа

## Назначение

`ColorQualityAuditV1` возвращает независимые сведения об экранном цвете и не
изменяет его. В модели нет универсальной оси `clean ↔ dirty`, вероятности красоты
или общего числового score.

Причина: разные исследования измеряют разные явления:

- координаты color appearance;
- vividness, blackness, whiteness, depth и clarity;
- brown induction;
- semantic `clean–dirty`;
- murky–clear и dull–vivid;
- preference и acceptability.

Корреляция между ними не делает их одним свойством.

## Appearance mode

Физически дисплей излучает свет, но интерпретация стимула зависит от композиции.
Один RGB может быть:

- самостоятельным светящимся UI-сигналом;
- поверхностью, показанной на экране;
- материалом после alpha-композиции;
- пространственным glow/blur;
- частью изображения.

Поэтому API требует явный `AppearanceMode`:

```rust
pub enum AppearanceMode {
    EmissiveUi,
    SurfaceLike,
    MaterialLike,
    SpatialEffect,
    ImageContent,
}
```

Mode не выводится из RGB, имени роли или темы. Исследование предметов, красок и
освещённых поверхностей не переносится на UI без отдельного основания.

## Контекст

`AppearanceContextV1` разделяет:

- `ViewingConditions` — адаптацию CAM16;
- измеренные `DisplayConditionsV1` — белый, чёрный и ambient в `cd/m²`;
- `SpatialContextV1` — adjacency, угловой размер и длительность;
- `AppearanceMode` — режим интерпретации.

`AppearanceContextV1::nominal` сохраняет отсутствие измерений как `None`. Номинал
не выдаётся за калиброванный дисплей.

## AppearanceProfileV1

Первая версия публикует только сведения с проверяемым происхождением:

```text
CAM16-UCS: J′, a′, b′, M′
radius_from_black = hypot(J′, M′)
chroma_angle      = atan2(M′, J′)
sRGB gamut geometry
```

`radius_from_black` — математическая геометрия выбранных координат, а не готовая
observer-шкала vividness. Опубликованные шкалы vividness/blackness/whiteness/depth
будут добавлены только после воспроизведения формул, reference vectors и проверки
applicability matrix из Issue #230.

## RelativeMutednessV2

Приглушённость сравнивается только с явно переданным семейным якорем. Абсолютная
граница «мутный/немутный» не вводится.

```text
loss = ln((M′/J′)_reference / (M′/J′)_candidate)
```

- `loss > 0` — candidate потерял относительную хроматичность;
- `loss = 0` — отношение сохранено;
- `loss < 0` — candidate стал относительно хроматичнее.

Ахроматические концы представлены enum-состояниями, а не `epsilon` и infinity.
Source anchor не сравнивается с чужим reference.

## WarmDarkInteraction V2

Историческая формула V2 сохранена как исследовательский baseline:

```text
y = max(0, b′ / hypot(J′, M′))
r = radius_foreground / radius_background
interaction = −e · y · r · ln(r)
```

Её неизменяемый идентификатор:

```text
lab-warm-dark-interaction-cam16ucs-v2
```

Это гипотеза Labpics, а не опубликованная шкала brownness или cleanliness.
Математически после выбора семейства `−r ln r` доказуемы его концы и максимум
при `r = 1/e`; сам выбор функции не выведен из observer data.

Результат возвращает только:

- `InteractionZero(reason)`;
- `InteractionPositive`;
- `InsufficientContext`;
- `NotApplicable(mode)`;
- `NumericallyIndeterminate`.

Слова `Clean` и `Dirty` запрещены: сколь угодно малый положительный член не является
человеческим категориальным ответом.

Чёрный background возвращает `InsufficientContext`, а не доказанный ноль. Данные
brown induction показывают, что часть наблюдателей способна видеть brown и без
более светлого непосредственного surround; текущего отношения радиусов для
универсального вывода недостаточно.

`SpatialEffect` и `ImageContent` не анализируются opaque-patch формулой. Для
`MaterialLike` без измеренного display/spatial context также возвращается
`InsufficientContext`.

## ColorQualityAuditV1

Агрегат содержит независимые поля:

```text
appearance
relative_mutedness? 
warm_dark_interaction
```

В нём нет weighted sum. Whiteness contamination и observer-validated brownness
будут отдельными сигналами, а не дополнительными коэффициентами одной формулы.

Audit не возвращает исправленный цвет и побайтно не влияет на эмиссию.

## Legacy

`crate::cleanliness` сохраняет «Закон Грязи V1» только для воспроизводимости
старых conformance-векторов. Legacy API:

- не является production policy;
- не участвует в `ColorQualityAuditV1`;
- не задаёт threshold;
- не разрешает автоматическую проекцию;
- сохраняет собственный неизменяемый смысл.

700 авторских отметок хранятся как falsification corpus Issue #225. Согласие с
одним автором не называется population accuracy.

## Проекция

Изменение цвета не входит в этот контракт. Будущая policy Issue #217 разделяет:

```text
Preserve — не менять bytes
Audit    — не менять bytes, вернуть отчёт
Project  — constrained finite solver с сертификатом
```

Core default — `Preserve`. Первый допустимый product default — `Audit`. `Project`
не включается до screen-native psychophysics Issue #232, global finite solver
Issue #218 и numerical proof Issue #223.

Если система выглядит плохо в `Preserve`, исправляется основной генератор. Проектор
не служит постобработкой, скрывающей дефект архитектуры.

## Инварианты

1. Один RGB с разными appearance mode остаётся разными запросами.
2. Номинальный и измеренный display context различимы.
3. Audit не изменяет исходные bytes.
4. Нет универсального cleanliness scalar.
5. Earthy, brown, olive и muted не считаются дефектом без противоположного intent.
6. Неполный контекст возвращает статус, а не правдоподобный fallback.
7. Каждый нестандартный сигнал несёт model id и provenance.
8. Новая observer-модель получает новый immutable id; старые версии не
   переопределяются.

## Источники

- C. Li et al., «Comprehensive color solutions: CAM16, CAT16, and CAM16-UCS»,
  *Color Research & Application* 42(6), 703–718 (2017),
  <https://doi.org/10.1002/col.22131>.
- R. S. Berns, «Extending CIELAB: Vividness, V*ab, Depth, D*ab, and Clarity,
  T*ab», *Color Research & Application* 39(4), 322–330 (2014),
  <https://doi.org/10.1002/col.21833>.
- Y. J. Cho et al., «A cross-cultural comparison of saturation, vividness,
  blackness and whiteness scales», *Color Research & Application* 42, 203–215
  (2017), <https://doi.org/10.1002/col.22065>.
- S. L. Buck et al., «Influence of surround proximity on induction of brown and
  darkness», *JOSA A* 33(3), A12–A21 (2016),
  <https://doi.org/10.1364/JOSAA.33.000A12>.
- T. Morimoto, E. Slezak, S. L. Buck, «No effects of surround complexity on
  brown induction», *JOSA A* 33(3), A45–A52 (2016),
  <https://doi.org/10.1364/JOSAA.33.000A45>.
- I. Kuriki, «Effect of material perception on mode of color appearance»,
  *Journal of Vision* 15(8):4 (2015).

Источники обосновывают координаты и необходимость разделять appearance phenomena.
Ни один из них не публикует формулу `−e·y·r·ln(r)` и не используется так, будто
публикует её.
