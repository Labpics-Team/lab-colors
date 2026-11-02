# Окончательный конечный растр

Профиль `ModeledFinalRasterV1` принимает два полных locked premultiplied RGBA8
источника, исполняет source-over и выдаёт окончательные opaque RGB8 bytes.
Декларацией, Program identity, вычислением, качеством и текущим допуском владеет Core.
Все поддержанные политики сохраняют источники и физическую композицию.
Входы фиксированы и задают единственный окончательный растр.
Вы явно выбираете `mandatory-exact-nominal-srgb8-v2`; Core автоматически допускает
этот растр, если техническая проверка пройдена и все пиксели соответствуют конвенции,
иначе отказывает. Множество допустимых результатов содержит этот растр либо пусто.
При успехе ближайший допустимый результат совпадает с исходной композицией:
политика сохраняет цвета входов и не ищет коррекцию.

Публичный вход — `finalRasterSourceOverWire(sourcePort, destinationPort, operator,
outputSlot, cleanliness)` и обычный `compileProgramWire`. Он использует V4 LCPW.
Point V1/V2 и computed assignment V3 сохраняют свои контракты.
Декларация содержит два premultiplied input port, source-over operator, один final
output slot и обязательный пятый аргумент:

| Значение | Действие Core | Нарушение конвенции |
| --- | --- | --- |
| `"off"` | Только техническая проверка полного opaque результата | Cleanliness не вычисляется |
| `"lint-exact-nominal-srgb8-v2"` | Проверка всех финальных пикселей и отчёт | Ready с неизменёнными bytes и подтверждёнными нарушениями |
| `"mandatory-exact-nominal-srgb8-v2"` | Обязательное соответствие опубликованному ExactNominalSrgb8CleanSetV2 | Failed с явным отказом |

Отсутствующий аргумент, `"auto"`, неизвестный профиль или политика получают
`program_field_unsupported`. Wire discriminant политики равен соответственно 0, 2
или 1. Неизвестные discriminants отвергаются. Старое обязательное требование
сохраняет bytes и Program identity; off и lint имеют собственную semantic identity.
Дубликаты и неправильные ссылки проверяет прежний компилятор.
Общий внутренний field binding `.new()` сохраняет прежнюю область вне этого
окончательного opaque профиля.

## Поддержка платформ

| Возможность | Rust Core | WASM/JS | UniFFI/Swift/Kotlin |
| --- | --- | --- | --- |
| Объявленная Cleanliness финального encoded-sRGB8 результата точки в Program V1–V3 | Поддержана | Поддержана | Поддержана |
| Полный растр V4 `ModeledFinalRasterV1`: ввод, текущий результат, потребление с проверкой допуска и off/lint/mandatory | Поддержан | Поддержан | Поддержан |

Swift и Kotlin используют `ProgramWireBuilderV4` и сгенерированный UniFFI-вход
`finalRasterWire`. `updateFinalRaster`, `finalRasterContentIdentity`,
`finalRasterRefusal` и `consumeFinalRaster` проецируют тот же публичный Core.
`updateObserved` остаётся входом point-наблюдений; для V4 он возвращает
`program_unsupported`. Классификация Cleanliness и текущий допуск принадлежат
одному Core; адаптеры передают входы и проецируют его результаты.

Нативный `ProgramFinalRasterCleanlinessRequirement` задаёт Off,
LintExactNominalSrgb8V2 или MandatoryExactNominalSrgb8V2. Callback
`ProgramFinalRasterConsumer.consume` получает owned `ProgramFinalRasterBytes`
с геометрией, DPR, ревизией, output slot, RGB8 bytes и типизированным
`ProgramFinalRasterCleanlinessAssessment`. Поле `requirement` сообщает политику,
`verdict` — NotApplied, Pass или Violation; counts, witness, profile и release
сохраняют описанный ниже смысл. Генератор UniFFI переводит имена вариантов
в соглашения Swift и Kotlin. Копию можно хранить после возврата callback и
освобождения native handle. Текущий допуск проверяет каждый новый вызов Core.

## Наблюдение и композиция

`runtime.updateFinalRaster(revision, width, height, dpr, context, sourcePort,
source, destinationPort, destination)` принимает полные row-major RGBA8 bytes
каждого port. Width/height — число device pixels. DPR — отдельный целочисленный
факт 1..4; пиксели не масштабируются. Context — непрозрачные 32 bytes
идентичности потребительской сцены, без утверждения о браузерном renderer.
Растр содержит от 1 до 65536 пикселей. Каждый массив имеет ровно
4 × width × height bytes. RGB каждого premultiplied пикселя ≤ alpha.

Входы и результат сохраняет immutable владелец той же Program generation и Session.
Admission включает полные bytes, геометрию, DPR и context. Повтор той же ревизии
идемпотентен только при полном совпадении. Изменённый payload той же ревизии,
старая ревизия, malformed input и resource failure сохраняют предыдущий commit.
Drop подготовленного перехода отменяет публикацию целиком.

Для каждого канала применяется существующий integer reference source-over:
premultiplied source + destination × (1 − source alpha), с прежним
`RoundHalfUpSrgb8V1`. Проверяется каждый окончательный alpha=255. Полностью
прозрачный foreground допустим, если финальный растр opaque. Это требование
действует и при off/lint; оно относится к физической области этого профиля.
Прямое доказательство полного растра не требует символической factorization
carrier contribution; прежний carrier proof сохраняет собственные условия.

## Отчёт Cleanliness

Lint и обязательный режим применяют один Core classifier
`ExactNominalSrgb8CleanSetV2` к каждому final RGB8 pixel. Полный fold даёт число
подтверждённых нарушений и первый row-major witness: pixel index, RGB и closed
rejected blue interval. `checkedPixelCount` равен width × height. Проверка первой
точки, среднего, centroid или N−1 пикселей не удостоверяет результат.

Callback `consumeFinalRaster` получает `frame.cleanliness`:

- `mode`: `"off"`, `"lint"` или `"mandatory"`;
- `verdict`: `"not-applied"`, `"pass"` или `"violation"`;
- `profile`: `"ExactNominalSrgb8CleanSetV2"` и `profileRelease`: копия 32-byte
  опубликованного release digest; при off оба равны null;
- `checkedPixelCount`, `violationCount` и `firstViolation`;
- `firstViolation` содержит `pixelIndex`, `composite` и `rejectedBlueInterval`;
  если нарушений нет, оно равно null.

Off сообщает `not-applied`, нулевые counts и null witness; это не оценка качества.
Lint может сообщить violation, сохраняя Ready и те же финальные bytes. Обязательный
режим выдаёт current frame только с pass, нулём нарушений и null witness.
Отчёт объясняет соответствие конкретной технической конвенции. Человеческое
восприятие, независимая human generalization и соответствие browser-host
renderer этим профилем не оцениваются. `firstViolation` описывает первый пример,
а `violationCount` — все нарушения; отдельный список всех пикселей не копируется.

## Текущий допуск и отказ

`runtime.consumeFinalRaster(revision, runtime.contentIdentity(), outputSlot,
context, consumer)` выполняется синхронно. До callback проверяются current Ready,
точные bindings и fresh TechnicalQuality с exact-reference replay.
При mandatory также требуется fresh CleanConvention в AUTH. Off не вызывает
Cleanliness classifier. Lint заново сопоставляет полный отчёт с текущими final
bytes; его нарушение не превращается в pass permit или обязательный отказ.
Отсутствие CC в off/lint не ослабляет требования mandatory.

Callback получает полные RGB8 bytes, width/height/DPR/revision/outputSlot, scope
`ModeledFinalRasterV1` и описанный отчёт. True подтверждает приём bytes.
Stale/unknown/failed, другая Program/output/context и отрицательный callback
не считаются потреблением. Во время callback обновление, чтение состояния и
повторное потребление этого runtime дают busy refusal. WASM также отвергает
`free` во время callback; native handles освобождаются по контракту UniFFI.

Transparent final pixel фиксирует Failed при любой политике. Нарушение
mandatory также фиксирует Failed. `runtime.finalRasterRefusal()` возвращает
точный отказ; lint-нарушение доступно в `frame.cleanliness`, без runtime refusal.
Нативный отказ сохраняет `code` и опциональные pixel index, RGB и interval.
После Failed потребление возвращает `program_field_stale` и не вызывает consumer.
Отрицательный ответ native consumer возвращает `program_field_consumer_rejected`,
его исключение — `program_field_consumer_failed`; оба сохраняют состояние Core.
Предыдущие bytes и скопированный frame остаются historical evidence без текущего
допуска. Изменение их копии не меняет Core. `assessmentReport()` для field
возвращает null: point-клетки не удостоверяют целый растр.

## Область и бюджет

Поддержан один source-over полного конечного растра. Произвольная непрерывная
анимация, непрерывное поле, blur/screen/lighter, computed raster search,
человеческое улучшение и browser-host conformance этим профилем не удостоверяются.
Неподдержанные объявления отвергаются, без перехода в point path.
Подбор RGB-назначений по объявленным областям, метрике и порядку выбора описан
в [контракте Program V3](program-assignment-v3.md).

65536 — публичный admission budget этого профиля. Исходные bytes занимают 8N,
reference scratch 36N, retained output 4N; scalar report и первый witness
добавляют постоянный объём. JS/WASM snapshot, consumer copy и удержанные поколения
добавляют собственную память. Проверка предела предшествует snapshot/copy.
Это не измерение resident memory/latency или allocator-wide recoverable OOM:
существующие Shared control blocks и холодная компиляция используют global allocator.
Производительность и WASM size проверяются на точных квалифицированных bytes.

Основания физического scope: W3C Compositing and Blending Level 1,
Candidate Recommendation Draft 21 марта 2024 года, §9.1.4 source-over:
https://www.w3.org/TR/compositing-1/#porterduffcompositingoperators_srcover.
WHATWG Canvas, Living Standard 30 октября 2026 года, pixel manipulation и
premultiplied alpha: https://html.spec.whatwg.org/multipage/canvas.html.
Переданные Core bytes и последующее rendering имеют разные границы: Canvas
ImageData использует собственные преобразования и может терять точность при
premultiply/unpremultiply. Consumer byte acceptance не даёт host или human evidence.
