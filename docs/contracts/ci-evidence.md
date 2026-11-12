# Источники проверок перед слиянием

Обязательный `CI` объединяет проверки одним нативным графом GitHub:
[PR и merge queue](../../.github/workflows/ci-gate.yml) вызывают Core, Native
и source admission. GitHub ждёт зависимости без занятого раннера; завершающая
job запускается и после отказа, отмены или пропуска producer.

Reusable workflows экспортируют только результаты jobs и такие же результаты
вложенных вызовов. Состав и полнота следуют из объявленных jobs и local `uses`
того же workflow snapshot: отдельного перечня имён checks нет. Пустой,
неполный, неизвестный или неуспешный результат не даёт зелёный `CI`.
Matrix проверяется через её нативный job result и существующие проверки
полноты producer-отчётов; наличие output отдельного shard не означает успех.
На PR формальная Kani-кампания отложена: её две matrix jobs и assembler имеют
единое условие `always() && github.event_name != 'pull_request'`. Очередь
проверяет все формальные harnesses и mutations общей комбинации до слияния;
push повторяет полную проверку точного исходника для публикации. Остальные
проверки PR сохраняются. Зелёный PR допускает вход в очередь и не утверждает,
что формальная квалификация уже выполнена.

CI worker передаёт результаты по именам. Проверяющая программа выводит для
каждой job ожидаемый `success` или разрешённый `skipped` из её условия и события;
непредусмотренный пропуск, успех отложенной job или их обмен дают отказ.
В очереди разрешён только `success`. Экспорт содержит только statuses, без
formal reports и distribution outputs. Его выражение автоматически выводится
из jobs; после изменения их состава обновите проекцию:

```sh
python3 scripts/ci/workflow_results.py project .github/workflows/ci-worker.yml
```

Программа отвергает устаревшую проекцию. Безусловные вложенные workers могут
использовать компактный список результатов, каждый из которых обязан быть
`success`. Диагностика остаётся отдельным ручным входом.
Контракт проверяет [workflow_results.py](../../scripts/ci/workflow_results.py).

Проверка PR не заменяет проверку общей комбинации в
[очереди GitHub](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
Отдельные push/main callers CI и Native сохраняют identity для публикации;
подписанные distribution attestations выполняются в push caller после проверки
producer. Формальные JSON-отчёты и SBOM не копируются в результат PR-графа.

Парные distribution-проверки transport/evaluate в
[CI worker](../../.github/workflows/ci-worker.yml) используют:

| Событие | Проверяемый исходник | Базовый исходник |
|---|---|---|
| `pull_request` | `pull_request.head.sha` | merge-base head и `pull_request.base.sha` |
| `merge_group` | `merge_group.head_sha` — вся комбинация | merge-base head и `merge_group.base_sha` |
| `push` | SHA события | Парное сравнение не выполняется |

До слияния оба CLI собираются из этих точных исходников. Отсутствие базы или
ошибка её сборки не превращается в одиночную проверку. Evaluate дополнительно
проверяет типизированный отказ при обмене запросами между различающимися
`runtimeConventionRelease`; эта проверка требует обеих сборок.

Метка PR `performance` включает отдельное
[измерение Program](../how-to/benchmark-program.md#сравнить-pr-с-его-базой-в-ci).
Очередь не наследует PR-метки и не включает эту диагностику автоматически.
Обязательные проверки пакета, размера WASM, результатов и ресурсов ядра
сохраняются независимо от неё. Включённое измерение нельзя считать
подтверждением производительности другой комбинации исходников.
