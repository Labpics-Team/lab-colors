# How-to: полнодоменное дуальное доказательство

Маршрут: один полнодоменный прогон обоих движков → 512 полос верификации
(256 на движок) → одна печать дуального доказательства.

Маршрут целиком в `main`. Проверено на `3ad5091e`.

Нужен `gh`, авторизованный в репозитории: все шаги — ручной
`workflow_dispatch`. Код проверяйте только в WSL или Linux (см. «Грабли»).

## 1. Прогон обоих движков на полном домене

```sh
gh workflow run full-domain-run.yml
gh run list --workflow full-domain-run.yml --limit 1 \
  --json databaseId --jq '.[0].databaseId'
```

Один прогон — две задачи, конверт каждой 480 мин. Замер прогона
`31116022208`: Arb 63 мин, MPFI 43 мин.

Выгружает по артефакту на движок из `evidence-out/`:
`verification-evidence-arb` и `verification-evidence-mpfi`. Идентификатор
прогона (дальше `EVIDENCE_RUN_ID`) — вход всех следующих шагов.

Проверять артефакты руками не нужно: допуск встроен в координатор (шаг 3).

## 2. План полос

Сеть не нужна, план лежит в коде:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode plan --out plan-out
# plan lanes=256 lane_width=65536 shard_width=16384 domain_points=16777216
```

Пишет `plan-out/dispatch-plan.json`. `--out` обязателен во всех режимах.

## 3. Диспатч полос — отдельно для каждого движка

Печать — поведение по умолчанию, сеть не трогается:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode verification-dispatch \
  --evidence-run-id EVIDENCE_RUN_ID \
  --evidence-artifact verification-evidence-arb \
  --out plan-out
```

Ровно 256 строк, окна от 0 до 16711680 шагом 65536. Пересчитайте строки.

Живой запуск — явный `--execute` с обязательным `--expect-lanes`:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode verification-dispatch \
  --evidence-run-id EVIDENCE_RUN_ID \
  --evidence-artifact verification-evidence-arb \
  --execute --expect-lanes 256 \
  --out plan-out
```

Повторите обе команды с `verification-evidence-mpfi`: всего 512 прогонов.

Отказы до первого прогона (все `exit 64`):

| Ситуация | Сообщение |
| --- | --- |
| `--execute` без `--expect-lanes` | `dispatch refused: --execute requires --expect-lanes` |
| `--expect-lanes 512` при плане на 256 | `dispatch refused: --expect-lanes=512 but the plan has 256 lanes` |
| `--mode plan --execute` | `plan mode cannot dispatch: drop --execute` |
| чужое `--evidence-artifact` | `lane dispatch rejected: ShardCorpusRejectedV1(...)` |

`--dry-run` удалён: argparse отвергает его с `exit 2`
(`unrecognized arguments: --dry-run`).

### Цепь допусков перед диспатчем

Только на живом пути, по возрастанию цены. Порядок — часть контракта.

1. **Происхождение прогона.** Путь `.github/workflows/full-domain-run.yml`,
   событие `workflow_dispatch`, статус `completed`, вывод `success`,
   читаемый 40-символьный `head_sha`. `head_sha` печатается оператору, а не
   решается здесь.
2. **Имя артефакта.** Прогон действительно несёт артефакт с этим именем, и тот
   не `expired`: истёкший всё ещё числится в листинге, но уже не скачивается.
   Принадлежность имени списку двух — отказ раньше и без сети (см. таблицу).
3. **Содержимое.** Скачивает артефакт целиком и требует внутри `job.bin` и
   `comparator-bundle/comparator-manifest-v2.bin`.

Происхождение раньше имени: имя ничего не говорит о том, откуда артефакт —
тот же workflow из форка публикует артефакт ровно допустимого имени.
Содержимое последним: это единственный шаг со скачиванием, и дешёвые отказы
не должны стоить сетевого вызова. Прогон, сделанный до нынешнего
`evidence-out/`, проходит проверки имени и умирал бы во всех 256 полосах на
первом `test -f`.

Полоса (`verification-lanes.yml`, конверт 480 мин) скачивает названный
артефакт из `EVIDENCE_RUN_ID`, проигрывает своё окно через `corpus_lane.py`
под компаратором движка и выгружает
`verification-lane-<evidence_artifact>-<window_start>-<window_points>`.

## 4. Сбор идентификаторов полос

Полосы носят `run-name` с координатами:

```
lane <evidence_artifact> <window_start>+<window_points> of <evidence_run_id>
```

Поэтому сбор — запрос, а не догадка по времени:

```sh
python3 proof/region/v1/corpus_dispatch.py --mode collect \
  --evidence-run-id EVIDENCE_RUN_ID \
  --evidence-artifact verification-evidence-arb \
  --out collect-arb
# collected lanes=256 artifact=verification-evidence-arb evidence_run=...
```

Пишет `collect-arb/lane-runs.json` схемы `corpus-lane-runs-v1`: `run_id` на
каждое окно. Имя файла одно, поэтому для второго движка нужен **другой**
`--out` — иначе он перезапишет первый.

Учитываются только прогоны с `conclusion=success`, чей титул называет тот же
артефакт и тот же `EVIDENCE_RUN_ID`. Две кампании пересекаются во времени по
построению, так что время не различитель.

Отказ несёт списки `missing` и `duplicated` окон; ничего не пишется —
частичный список неотличим от полного для того, кто его читает.

`--run-limit` (по умолчанию 2000) — глубина листинга. Правило насыщения:
если вернулось ровно `--run-limit` записей и покрытие неполно, отказ говорит,
что виноват может быть лимит, а не кампания — поднимите лимит прежде, чем
передиспатчить 256 полос.

Отказы аргументов (`exit 64`): непозитивный `--evidence-run-id`,
`--evidence-artifact` вне списка двух, непозитивный `--run-limit`.

## 5. Печать дуального доказательства

```sh
gh workflow run dual-proof.yml -f lane_run_ids=<id,id,...>
```

Вход `lane_run_ids` — идентификаторы полос **обоих** движков через запятую,
то есть объединение `run_id` из обоих `lane-runs.json` шага 4.

Одна задача, конверт 330 мин, пересобирает оба движка заново.

**Закон размещения.** `join_dual_proof_v1` требует две source-bound расписки,
а у них нет проводной формы by design: расписка, разбираемая из байтов,
позволила бы чужому коду чеканить происхождение. Значит держать обе может
только процесс, сам сминтивший обе, — одна задача, строящая и прогоняющая
оба движка подряд.

Полосы при этом приходят из прошлого прогона и всё равно принимаются: полоса
связывает *source*-идентичность компаратора, а она воспроизводится между
раннерами. Полная идентичность сворачивает наблюдение сборки и не
воспроизводится.

Покрытие проверяется до сборки: ровно две `comparator_source_identity`,
каждая покрывает `[0, 16777216)` без дыр и нахлёстов. Какому движку служит
полоса, решает source-идентичность внутри её манифеста, а не имя артефакта —
имя только держит файлы врозь.

Каждый прогон скачивается в свой каталог: две полосы, претендующие на одно
имя, — отказ, а не тихая победа одной.

Печатает `proof/region/v1/tests/dual_proof_gate.py`, результат — артефакт
`dual-proof-identity`.

## Открытые PR

Нет. `gh pr list --state open` пуст: маршрут целиком влит в `main`.

## Грабли

**Проверять на Windows нельзя.** Замер на `3ad5091e`: Windows — 361 тест и
7 ошибок загрузки, WSL — 515 тестов и 3 предсуществующих падения в
`test_build`. Причин у семи ошибок две, а не одна:

- 4 × `ModuleNotFoundError: No module named 'fcntl'` — `test_build`,
  `test_dual_proof`, `test_executor`, `test_mpfi_runtime`; корень —
  `proof/region/v1/executor.py:13`;
- 3 × `TypeError: invalid native process cwd` — `test_build_identity`,
  `test_corpus_shards`, `test_mpfi_build`; корень —
  `proof/region/v1/build/transport.py:715` строит модульную константу с
  `cwd="/"`, а на Windows `os.path.isabs("/")` — `False`, и проверка в
  `transport.py:588` отвергает её на импорте.

Discovery при этом молча отдаёт меньший набор. Проверяйте в WSL:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s proof/region/v1/tests -p "test_*.py"
python3 proof/region/v1/arb/tests/gate.py
python3 proof/region/v1/mpfi/tests/gate.py
```

CI гоняет второй режим, где `assert` исчезает: `ci-worker.yml:238` — discovery
под `PYTHONOPTIMIZE=2`, `arb.yml:41,48` — оба гейта под ним же. Прогоняйте оба
режима локально.

**Инвентарь тестов пинится в двух местах.**
`proof/region/v1/arb/tests/gate.py` держит `EXPECTED_TEST_INVENTORY_SHA256`,
а `proof/region/v1/tests/test_build.py` — собственные
`ARB_INVENTORY_SHA256_V1`, `ARB_ORDER_SHA256_V1` и `ARB_TEST_COUNT_V1`.
Второй набор — намеренно независимый внешний оракул: `test_build` импортирует
сам гейт (чтобы перечислить набор), но не его ожидаемый хеш. Поэтому
согласованная правка гейта проходит его собственную проверку и падает потом в
задаче `CI / test`.

Проверено: добавление одного теста и обновление **только** пина гейта даёт
гейт `OK (skipped=15)`, `exit 0` — и тут же
`AssertionError: 285 != 284` в `test_build.ExistingArbGateTests`, `exit 1`.
Так уже было в CI: прогон `31125436887` ветки PR 549, задача `CI / test`,
`AssertionError: 270 != 267` в `test_build.py:291`. На `3ad5091e` — 284 теста,
инвентарь `3284dccf286e90dcd07513c013b6994d619f0bb02232efee514fa08032301dbe`.

Обновляйте оба набора значениями из исполнения гейта в WSL:

```sh
python3 - <<'PY'
import hashlib, sys
sys.path.insert(0, "proof/region/v1/arb/tests")
import gate

ids = tuple(t.id() for t in gate.iter_tests_v1(gate.full_suite_v1()))
print("ARB_TEST_COUNT_V1      ", len(ids))
print("ARB_INVENTORY_SHA256_V1", gate.test_inventory_sha256_v1(gate.full_suite_v1()))
print("ARB_ORDER_SHA256_V1    ",
      hashlib.sha256(b"".join(i.encode("utf-8") + b"\n" for i in ids)).hexdigest())
PY
```

`ARB_INVENTORY_SHA256_V1` обязан совпасть с `EXPECTED_TEST_INVENTORY_SHA256`.

**Правка `crates/labcolors-core/src/lib.rs` ломает две аттестации.** Любая,
вплоть до комментария: обе считают точные байты файла. Проверено — один
дописанный комментарий даёт

```
clean-set verification: FAIL: receipt artifact[6] bytes do not match receipt metadata
point-support retained-surplus independent verification: FAIL: point-support semantic source drifted
```

Перегенерируйте четыре артефакта по цепочке, каждый следующий из предыдущего:

1. запись `crates/labcolors-core/src/lib.rs` (поля `bytes` и `sha256`) в
   `crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.json`;
2. `crates/labcolors-core/contracts/clean-set-srgb8-v1/receipt-v1.sha256`;
3. `EXPECTED_SOURCE_CAPSULE_SHA256` в `scripts/verify_point_support_surplus.py`;
4. `crates/labcolors-core/contracts/point-support-reference-surplus-q55-bps-proof-v1.json`
   — только через `python3 scripts/verify_point_support_surplus.py --emit`.

Проверьте обоими верификаторами:

```sh
python3 scripts/verify_clean_set_receipt.py product --product-root "$PWD"
python3 scripts/verify_point_support_surplus.py
```

Сверьте результат с `origin/main` поле за полем: меняться должны только
digest-ы над исходником. Ни одно число сертификата меняться не имеет права.
