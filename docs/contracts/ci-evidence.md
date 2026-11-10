# Источники проверок перед слиянием

Обязательный `CI` принимает только завершённые успешные workflows того же
события и SHA. Проверка PR не заменяет проверку общей комбинации в
[очереди GitHub](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue).
Состав проверок задаёт [peer_gate.py](../../scripts/ci/peer_gate.py).

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
