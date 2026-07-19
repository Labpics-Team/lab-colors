import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const ROOT = resolve(import.meta.dirname, "../../..");
const SELF = fileURLToPath(import.meta.url);
const PACKAGE_ROOT = join(ROOT, "packages", "colors");
const PACKAGE_MANIFEST = JSON.parse(
  readFileSync(join(PACKAGE_ROOT, "package.json"), "utf8"),
);
const RUNTIME_DOC_PATHS = [
  "packages/colors/README.md",
  ...PACKAGE_MANIFEST.files
    .filter((path) => /^(?:apply-theme|watch-theme|adapt-theme|effective-bg)\.(?:js|d\.ts)$/u.test(path))
    .map((path) => `packages/colors/${path}`),
];
const CLAIM_EXT = /\.(?:js|md|mjs|rs|ts)$/u;
const REPOSITORY_TEXT_EXT =
  /\.(?:c|cc|cpp|css|go|h|hpp|html|java|js|json|jsx|kt|md|mdx|mjs|py|rs|sh|swift|toml|ts|tsx|txt|ya?ml)$/u;
const CLAIM_SKIP = /(?:^|\/)(?:node_modules|pkg|target|\.git)(?:\/|$)|mutants\.out/u;
const HUMAN_CLEANLINESS_VERDICTS = [
  /Закон Грязи/u,
  /Muddiness Law/u,
  /0\s*[—-]\s*чистый,\s*1\s*[—-]\s*грязный/u,
  /оценка [«"]грязи[»"]/u,
];
const WHOLE_GLOW_CLAIM =
  /(?:glow[^.!?\n]*полного результата|полного результата[^.!?\n]*glow)/iu;
const FULL_SOLVE_EXACT_INVERSION_CLAIM =
  /(?:точн[а-яё]*\s+инверси[а-яё]*\s+прямого\s+пути|exact(?:ly)?\s+(?:inverts?|inversion\s+of)\s+the\s+(?:complete\s+)?forward\s+path)/iu;
const RETIRED_SENTIMENT_MODEL =
  /sentiment\.rs|`sentiments`|Sentiment(?:Curve|sConfig|Resolution)|LadderSource(?:::|Dto::)Sentiment|UnknownSentiment|resolve_config_sentiment_solid|WARNING_HUE_FLOOR_DEG|S_PERC_MIN|NeighborZone|Sticky Potential|Warning[- ]zone|brand[- ]displacement|achromatic sentiment/iu;
const RETIRED_SENTIMENT_SYMBOL =
  /\b(?:SentimentCategory(?:Dto)?|SentimentCurve|SentimentsConfig|SentimentsDto|SentimentResolution|UnknownSentiment|NeighborZone)\b|\b(?:LadderSource|LadderSourceDto)::Sentiment\b|\bSentiment\s*(?:\(|\{)|\b(?:pub\s+)?mod\s+sentiment\s*;|\b(?:compile_sentiment_tint|sentiment_solid_for_mode|sentiment_s_perc_min|resolve_sentiment_hue_among|resolve_config_sentiment_solid(?:_among)?|s_perc_min_(?:from_chromas|frozen))\b|\b(?:WARNING_HUE_FLOOR_DEG|S_PERC_MIN)\b|["'`]sentiments["'`]|\bsentiments\s*:|(?:\bkind|["'`]kind["'`])\s*:\s*["'`]sentiment["'`]/iu;

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

const RUNTIME_DOC_FALSE_CLAIMS = [
  {
    pattern: /правильные --lab-\* для своего фона/u,
    sample: "правильные --lab-* для своего фона",
    reason: "reference background estimate was promoted to a correct rendered result",
  },
  {
    pattern: /цвета будут корректны для наиболее сложного из них/u,
    sample: "цвета будут корректны для наиболее сложного из них",
    reason: "a finite sample set was promoted to the whole varying backdrop",
  },
  {
    pattern: /adaptTheme\(hero,[\s\S]{0,120}strict:\s*true/u,
    sample: "adaptTheme(hero, {\n  colors,\n  strict: true",
    reason: "legacy strict mode was presented as the recommended example",
  },
  {
    pattern: /~2\.5× на 3 сэмплах/u,
    sample: "~2.5× на 3 сэмплах",
    reason: "an ungated benchmark result was presented as a durable property",
  },
  {
    pattern: /measured\s+~2\.6x/iu,
    sample: "a measured ~2.6x on the multi-sample recheck",
    reason: "an ungated benchmark result was presented as a durable property",
  },
  {
    pattern: /перцептуально равномерная интерполяция между двумя hex-значениями/iu,
    sample: "перцептуально равномерная интерполяция между двумя hex-значениями",
    reason: "Oklab coordinate interpolation was promoted to a human-perception guarantee",
  },
  {
    pattern: /Perceptually uniform \(even crossfade timing/iu,
    sample: "Perceptually uniform (even crossfade timing",
    reason: "Oklab coordinate interpolation was promoted to a human-perception guarantee",
  },
  {
    pattern: /non-muddy/iu,
    sample: "non-muddy chroma path",
    reason: "Oklab coordinate interpolation was promoted to a cleanliness guarantee",
  },
  {
    pattern: /held legible against the hardest sample/iu,
    sample: "held legible against the hardest sample",
    reason: "tracked metrics were promoted to universal legibility",
  },
  {
    pattern: /hold while colours still pass/iu,
    sample: "hold while colours still pass",
    reason: "tracked metrics were promoted to universal legibility",
  },
  {
    pattern: /#287 owns the finite replacement/u,
    sample: "#287 owns the finite replacement",
    reason: "a closed Issue was used as public reference documentation",
  },
  {
    pattern: /replacement принадлежит #283/u,
    sample: "replacement принадлежит #283",
    reason: "a closed Issue was used as public reference documentation",
  },
  {
    pattern: /(?:Issue |#)(?:283|287)\b/iu,
    sample: "Issue #283 owns the replacement",
    reason: "a closed Issue was used as shipped runtime documentation",
  },
  {
    pattern: /(?:perceptually[- ]uniform|perceptually even|equal \*perceived\* change)/iu,
    sample: "perceptually even",
    reason: "coordinate interpolation was promoted to a perception guarantee",
  },
  {
    pattern: /(?:non-muddy|muddy desaturated midpoint)/iu,
    sample: "muddy desaturated midpoint",
    reason: "coordinate interpolation was promoted to a cleanliness guarantee",
  },
  {
    pattern: /(?:principled defaults|well under the flash threshold|more stressful than a soft one|imperceptible while|always legal)/iu,
    sample: "well under the flash threshold",
    reason: "runtime timing was promoted to an unregistered human or safety profile",
  },
  {
    pattern: /(?:held legible|keeps?[^\n]{0,80}legible|universal guarantee of legibility)/iu,
    sample: "colours are held legible",
    reason: "tracked metrics were promoted to universal legibility",
  },
  {
    pattern: /(?:фактическим фоном|surface is correct on creation)/iu,
    sample: "surface is correct on creation",
    reason: "a reference background estimate was promoted to an observed pixel",
  },
  {
    pattern: /(?:каждый кадр повторно проверяет|re-check each frame)/iu,
    sample: "re-check each frame",
    reason: "sample polling was confused with a metric recheck on unchanged state",
  },
  {
    pattern: /(?:one batched call per frame|per sample per frame)/iu,
    sample: "ONE batched call per frame",
    reason: "a performed metric recheck was promoted to an every-frame operation",
  },
  {
    pattern: /strongest the backdrop demands/iu,
    sample: "the strongest the backdrop demands",
    reason: "a provisional worst-sample heuristic was promoted to a final-field guarantee",
  },
  {
    pattern: /(?:auto-refresh on DOM attribute mutations|авто-обновление при DOM-мутациях)/iu,
    sample: "Auto-refresh on DOM attribute mutations",
    reason: "the style/class-only observer was promoted to arbitrary DOM mutations",
  },
  {
    pattern: /переключение темы[^\n]{0,120}MutationObserver[^\n]{0,40}автоматически/iu,
    sample: "переключение темы и style/class отслеживаются MutationObserver автоматически",
    reason: "an explicit setTheme operation was promoted to MutationObserver behaviour",
  },
  {
    pattern: /(?:currently-applied `--lab-\*`|текущие применённые --lab-\*)/iu,
    sample: "The currently-applied `--lab-*` variables",
    reason: "current() logical targets were described as painted DOM values",
  },
  {
    pattern: /endpoints are exact/iu,
    sample: "endpoints are exact",
    reason: "opaque RGB endpoint identity was promoted to alpha preservation",
  },
  {
    pattern: /~85-90%/u,
    sample: "measured at ~85-90% of the frame budget",
    reason: "an ungated benchmark snapshot was presented as a durable property",
  },
  {
    pattern: /commit-pinned гайде релиза 0\.10\.0/iu,
    sample: "commit-pinned гайде релиза 0.10.0",
    reason: "the public package linked to a private-repository migration guide",
  },
];
const MANUAL_VERIFICATION_PROSE = [
  /(?:docs\/)?verification-map\.md/u,
  /Карта верификации нижних слоёв/iu,
  /Каждая формула[\s\S]{0,240}ВНЕШНЕГО опубликованного эталона/iu,
  /\|\s*формула(?:\/инвариант)?\s*\|\s*чем верифицирован[а]?\s*\|\s*оракул\s*\|/iu,
  /Every vector here[\s\S]{0,160}(?:STANDARD|PEER-REVIEWED SOURCE)/iu,
  /These pin[\s\S]{0,200}STANDARDS\s*\/\s*PEER-REVIEWED SOURCES[\s\S]{0,120}not to the crate's own output/iu,
];
const LCS_LPC_DRIFT = [
  /Labpics Color Space/u,
  /Local Color State/u,
  /Local Perceptual Contrast/u,
  /LPC\s*=\s*APCA/iu,
  /APCA[^.\n]{0,160}под именем \*\*?LPC/iu,
  /^LPC\s*=\s*опубликованная контрастная кривая/imu,
  /J['′]\s*=\s*50[\s\S]{0,100}half-lightness/iu,
  /perceptually uniform J['′]\/M['′]/iu,
  /Because UCS is perceptually uniform/iu,
  /J['′]\s*[—-]\s*перцептуальн[а-яё]*\s+яркост/iu,
  /s:\s*f64[\s\S]{0,80}насыщенн/iu,
  /lab-colors\s+решает[^.]{0,200}перцептуальн[а-яё]*\s+пространств[а-яё]*\s+LCS/iu,
  /Perceptual-contrast core curve/iu,
  /generic perceptual-contrast math/iu,
  /метрика\s+называется\s+LPC/iu,
];
const YS_SCORE_OVERCLAIMS = [
  /signed\s+(?:perceptual\s+contrast|LPC)\b/iu,
  /знаков(?:ый|ая|ое|ого)\s+перцептивн[а-яё]*[^.!?\n]{0,48}\bLc\b/iu,
  /(?:perceptual\s+LPC\s+target|LPC\s+solution)/iu,
  /LPC[- ]перцептивн[а-яё]*\s+цел/iu,
  /(?:readability[- ](?:контраст|оценк)|ось\s+читаемости)/iu,
];
const YS_SCORE_CANONICAL_SURFACES = [
  {
    path: "crates/labcolors-core/src/solve.rs",
    patterns: [/signed Ys candidate score `Lc`/u, /not an admitted LPC\/readability certificate/iu],
  },
  {
    path: "crates/labcolors-core/src/semantic.rs",
    patterns: [/signed Ys candidate score `Lc`/u, /not[\s\S]{0,40}LPC\/readability verdict/iu],
  },
  {
    path: "crates/labcolors-wasm/src/lib.rs",
    patterns: [
      /Знаковая candidate-координата Ys \(`lc`\)/u,
      /не доказательство LPC или читаемости/iu,
    ],
  },
  {
    path: "crates/labcolors-wasm/src/dto.rs",
    patterns: [/signed Ys candidate score/iu, /not LPC\/readability evidence/iu],
  },
  {
    path: "crates/labcolors-ffi/src/lib.rs",
    patterns: [/кандидатная оценка `Lc` по `Ys`/u, /не LPC\/readability evidence/iu],
  },
  {
    path: "crates/labcolors-conformance/src/lib.rs",
    patterns: [/кандидатная оценка `Lc` по `Ys`/u, /не LPC\/readability evidence/iu],
  },
  {
    path: "packages/colors/README.md",
    patterns: [/кандидатная оценка по `Ys`/u, /не является LPC\/readability verdict/iu],
  },
];
const DISCARDED_HONEST_RESULT_CLAIMS = [
  /ADR[- ]?0002/iu,
  /honest-result-policy/iu,
  /nearest(?:[-\s]+)achievable/iu,
  /ближайш[а-яё]*\s+достижим[а-яё]*/iu,
  /human(?:-authored)?\s+input\s+(?:is|gets)\s+(?:silently\s+)?(?:coerced|clamped)/iu,
  /человеческ[а-яё]*\s+ввод\s+(?:тихо\s+)?(?:коэрс|кламп)[а-яё]*/iu,
  /ошибк[а-яё]*[^.!?\n]{0,80}человеческ[а-яё]*\s+ввод[^.!?\n]{0,80}(?:запрещен|недопустим)[а-яё]*/iu,
  /\bno on-grid colou?r\s+(?:reproduces?|can\s+reproduce|satisfies?|can\s+satisfy)\b/iu,
  /\bthe nearest on-grid colou?r\s+(?:reaches?|achieves?|is)\b/iu,
];
const DISCARDED_FAILURE_WIRE = [
  /\bquantization_gap\b/u,
  /["'`]?kind["'`]?\s*[:=]\s*["'`]unreachable\b/iu,
  /\b(?:Resolved|RoleOutcome|ColorError)::Unreachable\b/u,
  /\b(?:UnreachableRole|FailedRole)\b/u,
  /\bpub\s+enum\s+Unreachable\b/u,
  /\bunreachable_code\b/u,
  /\bpolarity_mismatch\b/u,
  /\bSolveFailure::PolarityMismatch\b/u,
];
const ORDINARY_UNREACHABLE_OUTPUT_DRIFT = [
  /локальн[а-яё]*\s+failure[^.!?\n]{0,160}доказанн[а-яё]*\s+недостижим[а-яё]*/iu,
  /FailureRole[\s\S]{0,320}недостижим[а-яё]*\s+\(`"unreachable"`\)/iu,
  /failure\s+отдельной\s+роли\s+—\s+\*\*часть\s+успешного\s+результата\*\*/iu,
  /В\s+успешном\s+снимке\s+`failure`\s+может\s+быть\s+только\s+`unreachable`\s+или\s+`unresolved`/iu,
  /недостижим[а-яё]*[^.!?\n]{0,100}часть\s+успешного\s+результата/iu,
];

function claimFiles(path, files = [], extensions = CLAIM_EXT) {
  if (!existsSync(path) || CLAIM_SKIP.test(path)) return files;
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) claimFiles(child, files, extensions);
    else if (extensions.test(entry.name)) files.push(child);
  }
  return files;
}

function maskRustNonCode(source) {
  // Все позиции ниже приходят из String API и потому измерены в UTF-16 code
  // units. split("") сохраняет ту же систему координат даже при astral chars.
  const masked = source.split("");
  const blank = (start, end) => {
    for (let index = start; index < end; index += 1) {
      if (masked[index] !== "\n" && masked[index] !== "\r") masked[index] = " ";
    }
  };
  let cursor = 0;
  while (cursor < source.length) {
    if (source.startsWith("//", cursor)) {
      const end = source.indexOf("\n", cursor + 2);
      const stop = end < 0 ? source.length : end;
      blank(cursor, stop);
      cursor = stop;
      continue;
    }
    if (source.startsWith("/*", cursor)) {
      let depth = 1;
      let end = cursor + 2;
      while (end < source.length && depth > 0) {
        if (source.startsWith("/*", end)) {
          depth += 1;
          end += 2;
        } else if (source.startsWith("*/", end)) {
          depth -= 1;
          end += 2;
        } else {
          end += 1;
        }
      }
      blank(cursor, end);
      cursor = end;
      continue;
    }
    const raw = /^(?:br|r)(#*)"/u.exec(source.slice(cursor));
    if (raw) {
      const close = `"${raw[1]}`;
      const contentStart = cursor + raw[0].length;
      const found = source.indexOf(close, contentStart);
      const end = found < 0 ? source.length : found + close.length;
      blank(cursor, end);
      cursor = end;
      continue;
    }
    const stringPrefix = source.startsWith('b"', cursor) ? 2 : source[cursor] === '"' ? 1 : 0;
    if (stringPrefix > 0) {
      let end = cursor + stringPrefix;
      while (end < source.length) {
        if (source[end] === "\\") end += 2;
        else if (source[end] === '"') {
          end += 1;
          break;
        } else end += 1;
      }
      blank(cursor, end);
      cursor = end;
      continue;
    }
    const character = /^(?:b)?'(?:\\.|[^'\\\r\n])'/u.exec(source.slice(cursor));
    if (character) {
      blank(cursor, cursor + character[0].length);
      cursor += character[0].length;
      continue;
    }
    cursor += 1;
  }
  return masked.join("");
}

function isProductionRustPath(file) {
  return /[\\/]src[\\/]/u.test(file);
}

function productionRustFiles() {
  return claimFiles(join(ROOT, "crates"), [], /\.rs$/u).filter(isProductionRustPath);
}

function productionPackageFiles() {
  return PACKAGE_MANIFEST.files
    .filter((path) => /\.(?:js|mjs|ts)$/u.test(path) && !path.startsWith("pkg/"))
    .map((path) => join(PACKAGE_ROOT, path))
    .filter(existsSync);
}

function knownFalseClaims(path, source) {
  const failures = [];
  // Regression laws from the false-claim cleanup: these are exact known lies,
  // not a vocabulary policy or a substitute for scientific review.
  if (/(^|[^0-9A-Fa-f])#89(?![0-9A-Fa-f])/u.test(source)) {
    failures.push(`${path}: #89 is not the Material owner`);
  }
  if (WHOLE_GLOW_CLAIM.test(source)) {
    failures.push(`${path}: point Glow evidence was promoted to a whole-effect claim`);
  }
  if (source.includes("labui-material.css")) {
    failures.push(`${path}: names a consumer that does not exist`);
  }
  if (/platform-characterized/iu.test(source)) {
    failures.push(`${path}: claims a stronger status than legacy-platform-dependent`);
  }
  if (FULL_SOLVE_EXACT_INVERSION_CLAIM.test(source)) {
    failures.push(`${path}: full solve was described as an exact inverse`);
  }
  if (
    /ADR[- ]?0003[^.!?\n]{0,160}(?:dormant|дормант|default[^.!?\n]{0,30}unchanged|дефолт[^.!?\n]{0,30}не измен)/iu.test(
      source,
    )
  ) {
    failures.push(`${path}: implemented Ys readability path was described as dormant`);
  }
  return failures;
}

function publicClaimFiles() {
  return [
    ...claimFiles(join(ROOT, "crates")),
    ...claimFiles(join(ROOT, "packages", "colors")),
    ...claimFiles(join(ROOT, "docs")),
    join(ROOT, "README.md"),
    join(ROOT, "conformance", "README.md"),
    join(ROOT, "bindings", "swift", "README.md"),
  ].filter((file) => file !== SELF);
}

function runtimeDocFalseClaims(path, source) {
  return RUNTIME_DOC_FALSE_CLAIMS
    .filter(({ pattern }) => pattern.test(source))
    .map(({ reason }) => `${path}: ${reason}`);
}

function manualVerificationProseResidue(path, source) {
  return MANUAL_VERIFICATION_PROSE
    .filter((pattern) => pattern.test(source))
    .map(
      () =>
        `${path}: hand-written verification prose competes with executable oracles`,
    );
}

function lcsLpcDrift(path, source) {
  return LCS_LPC_DRIFT.filter((pattern) => pattern.test(source)).map(
    () => `${path}: LCS/LPC brand or evidence boundary drifted`,
  );
}

function ysScoreOverclaim(path, source) {
  return YS_SCORE_OVERCLAIMS.filter((pattern) => pattern.test(source)).map(
    () => `${path}: frozen Ys candidate score was promoted to LPC/readability evidence`,
  );
}

function discardedHonestResultClaims(path, source) {
  return DISCARDED_HONEST_RESULT_CLAIMS
    .filter((pattern) => pattern.test(source))
    .map(
      () =>
        `${path}: discarded silent-coercion or global nearest-achievable doctrine`,
    );
}

function discardedFailureWire(path, source) {
  return DISCARDED_FAILURE_WIRE.filter((pattern) => pattern.test(source)).map(
    () => `${path}: discarded unreachable/quantization-gap machine contract`,
  );
}

function ordinaryUnreachableOutputDrift(path, source) {
  return ORDINARY_UNREACHABLE_OUTPUT_DRIFT
    .filter((pattern) => pattern.test(source))
    .map(
      () =>
        `${path}: ordinary Unreachable was promoted to a successful local role outcome`,
    );
}

test("production Rust path filter is separator-agnostic", () => {
  assert.equal(isProductionRustPath("/repo/crates/core/src/lib.rs"), true);
  assert.equal(isProductionRustPath(String.raw`C:\repo\crates\core\src\lib.rs`), true);
  assert.equal(isProductionRustPath("/repo/crates/core/tests/src_like.rs"), false);
});

test("false-claim detector bites without treating hex colours as Issue links", () => {
  assert.equal(knownFalseClaims("x.md", "см. #89").length, 1);
  assert.equal(knownFalseClaims("x.md", "цвета #89CFF0 и #8944AB").length, 0);
  assert.equal(knownFalseClaims("x.md", "Glow полного результата").length, 1);
  assert.equal(
    knownFalseClaims("x.md", "Glow описан как point layer.\nполного результата здесь нет.")
      .length,
    0,
  );
  assert.equal(knownFalseClaims("x.md", "потребляет labui-material.css").length, 1);
  assert.equal(knownFalseClaims("x.md", "platform-characterized").length, 1);
  assert.equal(
    knownFalseClaims("x.md", "ADR-0003 is dormant; default remains unchanged").length,
    1,
  );
  assert.equal(
    knownFalseClaims("x.md", "solve — точная инверсия прямого пути").length,
    1,
  );
  assert.equal(
    knownFalseClaims(
      "x.md",
      "Контрастное ядро инвертируется аналитически; эмитированный кандидат проверяется повторно.",
    ).length,
    0,
  );
});

test("runtime-doc detector bites on every rejected promotion", () => {
  for (const { sample, reason } of RUNTIME_DOC_FALSE_CLAIMS) {
    assert.ok(
      runtimeDocFalseClaims("x.md", sample).length >= 1,
      `detector did not bite: ${reason}`,
    );
  }
});

test("verification-index quarantine bites on links and renamed copies", () => {
  for (const sample of [
    "See docs/verification-map.md",
    "# Карта верификации нижних слоёв",
    "Каждая формула проверяется против ВНЕШНЕГО опубликованного эталона",
    "| формула/инвариант | чем верифицирована | оракул |",
    "Every vector here is a control point from a STANDARD or PEER-REVIEWED SOURCE",
    "These pin transforms to STANDARDS / PEER-REVIEWED SOURCES, not to the crate's own output",
  ]) {
    assert.equal(
      manualVerificationProseResidue("x.md", sample).length,
      1,
      `quarantine did not detect: ${sample}`,
    );
  }
});

test("repository claim scan includes every governed text format", () => {
  for (const extension of [
    "js",
    "jsx",
    "ts",
    "tsx",
    "py",
    "rs",
    "go",
    "java",
    "kt",
    "cpp",
    "h",
    "md",
    "mdx",
  ]) {
    assert.match(`claim.${extension}`, REPOSITORY_TEXT_EXT, extension);
  }
});

test("live repository has no hand-written global verification index", () => {
  const files = claimFiles(ROOT, [], REPOSITORY_TEXT_EXT).filter(
    (file) => file !== SELF,
  );
  const failures = files.flatMap((file) =>
    manualVerificationProseResidue(
      relative(ROOT, file),
      readFileSync(file, "utf8"),
    ),
  );
  assert.deepEqual(failures, []);
});

test("discarded honest-result doctrine detector bites without rejecting bounded truth", () => {
  for (const sample of [
    "ADR-0002 law 2",
    "docs/decisions/0002-honest-result-policy.md",
    "degraded to the nearest-achievable state",
    "возвращён ближайший достижимый цвет",
    "human-authored input is silently coerced",
    "человеческий ввод коэрсится по Постелу",
    "ошибки за человеческий ввод запрещены",
    "no on-grid colour reproduces it",
    "the nearest on-grid colour reaches only 7.85",
  ]) {
    assert.ok(
      discardedHonestResultClaims("x.md", sample).length >= 1,
      `detector did not bite: ${sample}`,
    );
  }
  for (const scopedTruth of [
    "invalid public input returns a typed error; it is never silently changed",
    "strict config parsing returns a typed error for an out-of-domain field",
    "the result has the lowest error among the three examined grid candidates",
    "opaque endpoint returned with a typed degraded status",
  ]) {
    assert.deepEqual(discardedHonestResultClaims("x.md", scopedTruth), []);
  }
});

test("live repository has no discarded honest-result doctrine", () => {
  const files = claimFiles(ROOT, [], REPOSITORY_TEXT_EXT).filter(
    (file) => file !== SELF,
  );
  const failures = files.flatMap((file) =>
    discardedHonestResultClaims(
      relative(ROOT, file),
      readFileSync(file, "utf8"),
    ),
  );
  assert.deepEqual(failures, []);
  assert.equal(
    existsSync(join(ROOT, "docs/decisions/0002-honest-result-policy.md")),
    false,
    "the contradictory ADR must not survive as an empty or rewritten live file",
  );
});

test("discarded failure wire detector rejects every old machine shape", () => {
  for (const sample of [
    '"code":"quantization_gap"',
    'readonly kind: "unreachable"',
    "Resolved::Unreachable(reason)",
    "RoleOutcome::Unreachable { code }",
    "ColorError::Unreachable { code }",
    "export interface UnreachableRole",
    "export interface FailedRole",
    "pub enum Unreachable",
    "fn unreachable_code(reason: Unreachable)",
    '"code":"polarity_mismatch"',
    "SolveFailure::PolarityMismatch { target }",
  ]) {
    assert.ok(
      discardedFailureWire("x.rs", sample).length >= 1,
      `failure-wire detector did not bite: ${sample}`,
    );
  }
});

test("live repository has no discarded failure wire", () => {
  const files = claimFiles(ROOT, [], REPOSITORY_TEXT_EXT).filter(
    (file) => file !== SELF,
  );
  const failures = files.flatMap((file) =>
    discardedFailureWire(relative(ROOT, file), readFileSync(file, "utf8")),
  );
  assert.deepEqual(failures, []);
});

test("ordinary Unreachable output detector bites without rejecting Glow target status", () => {
  for (const sample of [
    "локальный failure: доказанная недостижимость",
    '`FailureRole`: category отделяет доказанную недостижимость (`"unreachable"`)',
    "failure отдельной роли — **часть успешного результата**",
    "В успешном снимке `failure` может быть только `unreachable` или `unresolved`",
    "пер-ролевая недостижимость — часть успешного результата",
  ]) {
    assert.ok(
      ordinaryUnreachableOutputDrift("x.md", sample).length >= 1,
      `detector did not bite: ${sample}`,
    );
  }
  for (const lawful of [
    'targetStatus: "exact-noop-unreachable"',
    'targetStatus: "legacy-unreachable"',
    "ordinary Unreachable rejects the whole resolve as OutputConflictError",
    "Unresolved remains a successful local bounded-search outcome",
  ]) {
    assert.deepEqual(ordinaryUnreachableOutputDrift("x.md", lawful), []);
  }
});

test("public output docs do not admit ordinary Unreachable as partial success", () => {
  const paths = [
    "README.md",
    ...RUNTIME_DOC_PATHS,
    "crates/labcolors-wasm/src/engine.rs",
    "crates/labcolors-wasm/src/lib.rs",
  ];
  const failures = paths.flatMap((path) =>
    ordinaryUnreachableOutputDrift(path, readFileSync(join(ROOT, path), "utf8")),
  );
  assert.deepEqual(failures, []);
});

test("LCS/LPC drift detector bites on every rejected expansion or reduction", () => {
  for (const sample of [
    "LCS means Labpics Color Space",
    "LCS means Local Color State",
    "LPC means Local Perceptual Contrast",
    "LPC = APCA + H-K",
    "APCA реализована под именем **LPC**",
    "LPC = опубликованная контрастная кривая",
    "J'=50 reads as half-lightness",
    "maps correlates onto perceptually uniform J'/M'",
    "Because UCS is perceptually uniform, this is a human scale",
    "J' — перцептуальная яркость (CAM16-UCS)",
    "s: f64, // насыщенность = M' / (J' + 1)",
    "lab-colors решает её в собственном перцептуальном пространстве LCS",
    "Perceptual-contrast core curve",
    "Faithful port of the generic perceptual-contrast math",
    "метрика называется LPC",
  ]) {
    assert.ok(lcsLpcDrift("x.md", sample).length >= 1, `detector did not bite: ${sample}`);
  }
});

test("Ys candidate-score detector bites on known evidence promotions", () => {
  for (const sample of [
    "signed perceptual contrast Lc",
    "знаковый перцептивный контраст Lc",
    "perceptual LPC target",
    "LPC solution",
    "LPC-перцептивная цель",
    "readability-контраст",
    "ось читаемости",
  ]) {
    assert.ok(ysScoreOverclaim("x.md", sample).length >= 1, `detector did not bite: ${sample}`);
  }
  for (const scopedTruth of [
    "signed Ys candidate score (`lc`) from the frozen SAPC-shaped curve",
    "`lc` is not LPC/readability evidence",
    "WCAG ratio is reported independently",
  ]) {
    assert.deepEqual(ysScoreOverclaim("x.md", scopedTruth), []);
  }
});

test("live public surfaces do not promote the frozen Ys score", () => {
  const failures = publicClaimFiles().flatMap((file) =>
    ysScoreOverclaim(relative(ROOT, file), readFileSync(file, "utf8")),
  );
  assert.deepEqual(failures, []);
});

test("every shipped lc surface carries the canonical scope marker", () => {
  for (const { path, patterns } of YS_SCORE_CANONICAL_SURFACES) {
    const source = readFileSync(join(ROOT, path), "utf8");
    for (const pattern of patterns) {
      assert.match(source, pattern, `${path} lost the Ys candidate-score scope marker`);
    }
  }
});

test("live repository keeps canonical LCS/LPC names and evidence boundaries", () => {
  const files = claimFiles(ROOT, [], REPOSITORY_TEXT_EXT).filter(
    (file) => file !== SELF,
  );
  const failures = files.flatMap((file) =>
    lcsLpcDrift(relative(ROOT, file), readFileSync(file, "utf8")),
  );
  assert.deepEqual(failures, []);

  assert.match(
    readFileSync(join(ROOT, "crates/labcolors-core/src/lcs.rs"), "utf8"),
    /Labpics Colors Space/u,
  );
  assert.match(
    readFileSync(join(ROOT, "crates/labcolors-core/src/lpc.rs"), "utf8"),
    /Labpics Perceptual Contrast/u,
  );
});

test("runtime docs do not promote estimates, samples, or coordinates", () => {
  const paths = [
    ...RUNTIME_DOC_PATHS,
    "crates/labcolors-wasm/src/lib.rs",
  ];
  const failures = paths.flatMap((path) =>
    runtimeDocFalseClaims(path, readFileSync(join(ROOT, path), "utf8")),
  );
  assert.deepEqual(failures, []);
});

const RESOLVED_THEME_FIELDS = new Map([
  ["theme", "ThemeName"],
  ["background", "string"],
  ["vars", "Readonly<Record<string, string>>"],
  ["roles", "Readonly<Record<string, RoleResult>>"],
]);

function parseTypes(source, fileName) {
  return ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
}

function namedTopLevelDeclarations(sourceFile, expectedName) {
  return sourceFile.statements.flatMap((node) => {
    if (ts.isImportDeclaration(node)) {
      const clause = node.importClause;
      if (clause === undefined) return [];
      const bindings = clause.name === undefined ? [] : [clause.name];
      if (clause.namedBindings !== undefined) {
        if (ts.isNamespaceImport(clause.namedBindings)) {
          bindings.push(clause.namedBindings.name);
        } else {
          bindings.push(...clause.namedBindings.elements.map((element) => element.name));
        }
      }
      return bindings.filter((binding) => binding.text === expectedName);
    }
    if (node.name && ts.isIdentifier(node.name) && node.name.text === expectedName) {
      return [node];
    }
    if (!ts.isVariableStatement(node)) return [];
    return node.declarationList.declarations.filter(
      (declaration) =>
        ts.isIdentifier(declaration.name) && declaration.name.text === expectedName,
    );
  });
}

function namedResolvedThemeDeclarations(sourceFile) {
  return namedTopLevelDeclarations(sourceFile, "ResolvedTheme");
}

function resolvedThemeShapeFailures(source, fileName, label, requireExport) {
  const failures = [];
  const sourceFile = parseTypes(source, fileName);
  if (sourceFile.parseDiagnostics.length > 0) failures.push(`${label} syntax`);
  for (const utility of ["Readonly", "Record"]) {
    if (namedTopLevelDeclarations(sourceFile, utility).length > 0) {
      failures.push(`${label} shadowed ${utility}`);
    }
  }
  const declarations = sourceFile.statements.filter(
    (node) => ts.isInterfaceDeclaration(node) && node.name.text === "ResolvedTheme",
  );
  if (declarations.length !== 1) return [`${label} interface`];
  const declaration = declarations[0];
  if (namedResolvedThemeDeclarations(sourceFile).length !== 1) {
    failures.push(`${label} merged declaration`);
  }
  if (
    requireExport &&
    !declaration.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword)
  ) {
    failures.push(`${label} export`);
  }
  if (declaration.modifiers?.some((modifier) => modifier.kind === ts.SyntaxKind.DefaultKeyword)) {
    failures.push(`${label} default export`);
  }
  if ((declaration.heritageClauses?.length ?? 0) > 0) {
    failures.push(`${label} heritage`);
  }
  if ((declaration.typeParameters?.length ?? 0) > 0) {
    failures.push(`${label} type parameters`);
  }

  const members = new Map();
  for (const member of declaration.members) {
    const name = ts.isPropertySignature(member) && ts.isIdentifier(member.name)
      ? member.name.text
      : null;
    if (name === null || members.has(name)) {
      failures.push(`${label} unexpected member`);
      continue;
    }
    members.set(name, member);
  }
  for (const [name, expectedType] of RESOLVED_THEME_FIELDS) {
    const member = members.get(name);
    const readonly = member?.modifiers?.some(
      (modifier) => modifier.kind === ts.SyntaxKind.ReadonlyKeyword,
    );
    if (
      member === undefined ||
      !readonly ||
      member.questionToken !== undefined ||
      member.type?.getText(sourceFile) !== expectedType
    ) {
      failures.push(`${label} ${name}`);
    }
    members.delete(name);
  }
  for (const name of members.keys()) {
    failures.push(`${label} unexpected ${name}`);
  }
  return failures;
}

function resolvedThemeContractFailures(indexTypes, declarationTypes, readme) {
  const failures = [];
  const index = parseTypes(indexTypes, "index.d.ts");
  const exported = index.statements.flatMap((node) => {
    if (
      !ts.isExportDeclaration(node) ||
      !ts.isStringLiteral(node.moduleSpecifier) ||
      node.moduleSpecifier.text !== "./pkg/labcolors.js" ||
      !node.exportClause ||
      !ts.isNamedExports(node.exportClause)
    ) {
      return [];
    }
    return node.exportClause.elements.flatMap((element) => {
      const sourceName = element.propertyName?.text ?? element.name.text;
      return sourceName === "ResolvedTheme" && element.name.text === "ResolvedTheme"
        ? ["ResolvedTheme"]
        : [];
    });
  });
  if (exported.length !== 1) failures.push("index export");

  failures.push(
    ...resolvedThemeShapeFailures(
      declarationTypes,
      "labcolors.d.ts",
      "declaration",
      true,
    ),
  );
  const readmeBlocks = [...readme.matchAll(/```ts\n([\s\S]*?)\n```/gu)]
    .map((match, index) => ({
      source: match[1],
      fileName: `README-${index}.ts`,
    }))
    .filter(({ source, fileName }) =>
      namedResolvedThemeDeclarations(parseTypes(source, fileName)).length > 0
    );
  if (readmeBlocks.length !== 1) failures.push("README code block count");
  else {
    failures.push(
      ...resolvedThemeShapeFailures(
        readmeBlocks[0].source,
        readmeBlocks[0].fileName,
        "README",
        false,
      ),
    );
  }
  return failures;
}

function replaceResolvedThemeDeclarationField(source, field, replacement) {
  const sourceFile = parseTypes(source, "labcolors.d.ts");
  const declaration = sourceFile.statements.find(
    (node) => ts.isInterfaceDeclaration(node) && node.name.text === "ResolvedTheme",
  );
  assert.ok(declaration, "fixture must contain the generated ResolvedTheme declaration");
  const start = declaration.getStart(sourceFile);
  const body = source.slice(start, declaration.end);
  assert.ok(body.includes(field), `ResolvedTheme fixture field not found: ${field}`);
  return source.slice(0, start) + body.replace(field, replacement) + source.slice(declaration.end);
}

function replaceResolvedThemeReadmeField(readme, field, replacement) {
  const block = [...readme.matchAll(/```ts\n([\s\S]*?)\n```/gu)]
    .map((match) => match[1])
    .find((candidate) => candidate.includes("interface ResolvedTheme"));
  assert.ok(block, "fixture must contain the README ResolvedTheme code block");
  assert.ok(block.includes(field), `README fixture field not found: ${field}`);
  return readme.replace(block, block.replace(field, replacement));
}

test("ResolvedTheme contract guard bites at every public SSOT boundary", () => {
  const indexTypes = readFileSync(join(ROOT, "packages/colors/index.d.ts"), "utf8");
  const declarationTypes = readFileSync(
    join(ROOT, "packages/colors/pkg/labcolors.d.ts"),
    "utf8",
  );
  const readme = readFileSync(join(ROOT, "packages/colors/README.md"), "utf8");
  assert.deepEqual(resolvedThemeContractFailures(indexTypes, declarationTypes, readme), []);

  assert.deepEqual(
    resolvedThemeContractFailures(
      indexTypes.replace("  ResolvedTheme,", "  // ResolvedTheme,"),
      declarationTypes,
      readme,
    ),
    ["index export"],
  );
  assert.deepEqual(
    resolvedThemeContractFailures(
      indexTypes.replace("  ResolvedTheme,", "  RoleResult as ResolvedTheme,"),
      declarationTypes,
      readme,
    ),
    ["index export"],
  );
  for (const [name, type] of RESOLVED_THEME_FIELDS) {
    const field = `readonly ${name}: ${type};`;
    assert.ok(
      resolvedThemeContractFailures(
        indexTypes,
        replaceResolvedThemeDeclarationField(declarationTypes, field, `// ${field}`),
        readme,
      ).includes(`declaration ${name}`),
      `generated declaration mutation must bite: ${name}`,
    );
    assert.ok(
      resolvedThemeContractFailures(
        indexTypes,
        declarationTypes,
        replaceResolvedThemeReadmeField(readme, field, `// ${field}`),
      ).includes(`README ${name}`),
      `README mutation must bite: ${name}`,
    );
  }
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      replaceResolvedThemeDeclarationField(
        declarationTypes,
        "readonly roles: Readonly<Record<string, RoleResult>>;",
        "readonly roles: Readonly<Record<string, RoleResult>>;\n    readonly revision: string;",
      ),
      readme,
    ).includes("declaration unexpected revision"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      replaceResolvedThemeReadmeField(
        readme,
        "readonly roles: Readonly<Record<string, RoleResult>>;",
        "readonly roles: Readonly<Record<string, RoleResult>>;\n  readonly revision: string;",
      ),
    ).includes("README unexpected revision"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "interface ExtraResolvedTheme { readonly revision: string; }\n" +
          "export interface ResolvedTheme extends ExtraResolvedTheme {",
      ),
      readme,
    ).includes("declaration heritage"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace(
        "interface ResolvedTheme {",
        "interface ExtraResolvedTheme { readonly revision: string; }\n" +
          "interface ResolvedTheme extends ExtraResolvedTheme {",
      ),
    ).includes("README heritage"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "export declare class ResolvedTheme { readonly revision: string; }\n" +
          "export interface ResolvedTheme {",
      ),
      readme,
    ).includes("declaration merged declaration"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace(
        "interface ResolvedTheme {",
        "declare class ResolvedTheme { readonly revision: string; }\n" +
          "interface ResolvedTheme {",
      ),
    ).includes("README merged declaration"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "export declare const ResolvedTheme: { readonly revision: string };\n" +
          "export interface ResolvedTheme {",
      ),
      readme,
    ).includes("declaration merged declaration"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace(
        "interface ResolvedTheme {",
        "declare const ResolvedTheme: { readonly revision: string };\n" +
          "interface ResolvedTheme {",
      ),
    ).includes("README merged declaration"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "export default interface ResolvedTheme {",
      ),
      readme,
    ).includes("declaration default export"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace(
        "interface ResolvedTheme {",
        "export default interface ResolvedTheme {",
      ),
    ).includes("README default export"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "type Readonly<T> = T;\nexport interface ResolvedTheme {",
      ),
      readme,
    ).includes("declaration shadowed Readonly"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace(
        "interface ResolvedTheme {",
        "type Readonly<T> = T;\ninterface ResolvedTheme {",
      ),
    ).includes("README shadowed Readonly"),
  );
  for (const utility of ["Readonly", "Record"]) {
    const imports = [
      `import ${utility} from "./hostile.js";`,
      `import { Hostile as ${utility} } from "./hostile.js";`,
      `import * as ${utility} from "./hostile.js";`,
    ];
    for (const hostileImport of imports) {
      assert.ok(
        resolvedThemeContractFailures(
          indexTypes,
          declarationTypes.replace(
            "export interface ResolvedTheme {",
            `${hostileImport}\nexport interface ResolvedTheme {`,
          ),
          readme,
        ).includes(`declaration shadowed ${utility}`),
        `generated declaration must reject ${hostileImport}`,
      );
      assert.ok(
        resolvedThemeContractFailures(
          indexTypes,
          declarationTypes,
          readme.replace(
            "interface ResolvedTheme {",
            `${hostileImport}\ninterface ResolvedTheme {`,
          ),
        ).includes(`README shadowed ${utility}`),
        `README must reject ${hostileImport}`,
      );
    }
  }
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes.replace(
        "export interface ResolvedTheme {",
        "export interface ResolvedTheme<T> {",
      ),
      readme,
    ).includes("declaration type parameters"),
  );
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      readme.replace("interface ResolvedTheme {", "interface ResolvedTheme<T> {"),
    ).includes("README type parameters"),
  );
  const readmeResolvedThemeBlock = [...readme.matchAll(/```ts\n([\s\S]*?)\n```/gu)]
    .map((match) => match[0])
    .find((block) => block.includes("interface ResolvedTheme"));
  assert.ok(readmeResolvedThemeBlock);
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      `${readmeResolvedThemeBlock}\n${readme}`,
    ).includes("README code block count"),
  );
  const driftedReadme = replaceResolvedThemeReadmeField(
    readme,
    "readonly roles: Readonly<Record<string, RoleResult>>;",
    "readonly roles: Readonly<Record<string, RoleResult>>;\n  readonly revision: string;",
  ).replace("interface ResolvedTheme {", "interface /* hostile */ ResolvedTheme {");
  assert.ok(
    resolvedThemeContractFailures(
      indexTypes,
      declarationTypes,
      `${readmeResolvedThemeBlock}\n${driftedReadme}`,
    ).includes("README code block count"),
  );
});

test("runtime docs scope background evidence to estimates and finite samples", () => {
  const readme = readFileSync(join(ROOT, "packages/colors/README.md"), "utf8");
  const adaptTypes = readFileSync(
    join(ROOT, "packages/colors/adapt-theme.d.ts"),
    "utf8",
  );
  const backgroundTypes = readFileSync(
    join(ROOT, "packages/colors/effective-bg.d.ts"),
    "utf8",
  );
  const watchSource = readFileSync(
    join(ROOT, "packages/colors/watch-theme.js"),
    "utf8",
  );
  const watchTypes = readFileSync(
    join(ROOT, "packages/colors/watch-theme.d.ts"),
    "utf8",
  );

  assert.match(readme, /конечный набор[^\n]*образц/u);
  assert.match(readme, /только[^\n]*переданн[^\n]*точ/u);
  assert.match(adaptTypes, /finite, caller-supplied sample set/iu);
  assert.match(adaptTypes, /does not infer[^\n]*between samples/iu);
  assert.match(backgroundTypes, /reference estimate/iu);
  assert.match(backgroundTypes, /solid\/translucent ancestor/iu);
  assert.match(backgroundTypes, /`background-color` chain/iu);
  assert.match(backgroundTypes, /not[\s*]+a browser pixel observation/iu);
  assert.match(backgroundTypes, /alpha[^\n]*discarded/iu);
  assert.match(adaptTypes, /Канонические логические цели/u);
  assert.match(watchSource, /reference estimate/iu);
  assert.match(readme, /изменения атрибутов `style`\/`class`/iu);
  for (const source of [watchSource, watchTypes]) {
    assert.match(source, /`style`\/`class` attribute changes/iu);
  }
  for (const source of [readme, watchSource, watchTypes]) assert.match(source, /setTheme/iu);
  assert.match(
    watchSource,
    /attributeFilter:\s*\["style", "class"\]/u,
    "observer implementation and public scope must name the same two attributes",
  );
  assert.equal(
    existsSync(join(ROOT, "docs/migrations/exact-alpha-glow.md")),
    false,
    "obsolete private-link migration stub must not survive in the live tree",
  );
});

test("cleanliness-verdict quarantine bites on every rejected public meaning", () => {
  for (const claim of [
    "Закон Грязи",
    "Muddiness Law",
    "0 — чистый, 1 — грязный",
    "оценка «грязи»",
  ]) {
    assert.equal(
      HUMAN_CLEANLINESS_VERDICTS.some((pattern) => pattern.test(claim)),
      true,
      `quarantine did not detect: ${claim}`,
    );
  }
});

test("known false Material/Glow claims stay absent from public surfaces", () => {
  const files = publicClaimFiles();
  const failures = files.flatMap((file) =>
    knownFalseClaims(relative(ROOT, file), readFileSync(file, "utf8")),
  );
  assert.deepEqual(failures, []);
});

test("public claim inventory includes the shipped Swift README", () => {
  assert.ok(publicClaimFiles().includes(join(ROOT, "bindings", "swift", "README.md")));
});

test("legacy cleanliness proxy is excised, not quarantined", () => {
  // Инвариант: карантин заменён вырезом. Ни модуль, ни векторное семейство не
  // существуют; публичные поверхности не несут ни API, ни человеческих
  // вердиктов чистоты.
  assert.equal(
    existsSync(join(ROOT, "crates/labcolors-core/src/cleanliness.rs")),
    false,
    "cleanliness.rs must stay deleted",
  );
  assert.equal(
    existsSync(join(ROOT, "conformance/vectors/muddiness.json")),
    false,
    "muddiness family must stay deleted",
  );

  // Закон удаления пака (pack_v10_removes_only_the_muddiness_family) легально
  // НАЗЫВАЕТ удалённое семейство — поэтому по conformance-крейту запрещаются
  // только API-идентификаторы прокси, по остальным поверхностям — само слово.
  const surfaces = [
    ["crates/labcolors-wasm/src/lib.rs", /muddiness|drab|n_pure/iu],
    ["crates/labcolors-ffi/src/lib.rs", /muddiness|drab|n_pure/iu],
    [
      "crates/labcolors-conformance/src/lib.rs",
      /muddiness_from_hex|MuddinessVector|generate_muddiness|\bdrab\b|n_pure/iu,
    ],
    ["packages/colors/README.md", /muddiness|drab|n_pure/iu],
    ["packages/colors/index.d.ts", /muddiness|drab|n_pure/iu],
    ["bindings/swift/README.md", /muddiness|drab|n_pure/iu],
  ];
  const publicText = surfaces
    .map(([path, forbidden]) => {
      const source = readFileSync(join(ROOT, path), "utf8");
      assert.doesNotMatch(
        source,
        forbidden,
        `${path}: excised proxy identifiers must not resurface`,
      );
      return source;
    })
    .join("\n");

  for (const forbidden of HUMAN_CLEANLINESS_VERDICTS) {
    assert.doesNotMatch(publicText, forbidden);
  }
});

test("legacy sentiment curve is excised instead of preserved as schema", () => {
  for (const sample of [
    "sentiment.rs",
    "`sentiments`",
    "SentimentCurve",
    "LadderSource::Sentiment",
    "WARNING_HUE_FLOOR_DEG",
    "Warning-zone",
    "brand-displacement",
  ]) {
    assert.match(sample, RETIRED_SENTIMENT_MODEL, `detector did not bite: ${sample}`);
  }
  for (const sample of [
    "SentimentCategoryDto",
    "SentimentCurve",
    "SentimentsConfig",
    "LadderSource::Sentiment",
    "enum LadderSource { Sentiment(String) }",
    "enum LadderSourceDto { Sentiment { name: String } }",
    "pub mod sentiment;",
    "resolve_sentiment_hue_among",
    "WARNING_HUE_FLOOR_DEG",
    '"sentiments"',
    'kind: "sentiment"',
  ]) {
    assert.match(
      sample,
      RETIRED_SENTIMENT_SYMBOL,
      `production-symbol detector did not bite: ${sample}`,
    );
  }

  assert.equal(
    existsSync(join(ROOT, "crates/labcolors-core/src/sentiment.rs")),
    false,
    "the legacy sentiment solver module must stay deleted",
  );

  const surfaces = [
    [
      "crates/labcolors-core/src/config.rs",
      /\b(?:SentimentCategory|SentimentsConfig|UnknownSentiment|SentimentResolution)\b|LadderSource::Sentiment|resolve_config_sentiment_solid/gu,
    ],
    [
      "crates/labcolors-core/src/lib.rs",
      /pub mod sentiment|\b(?:SentimentCategory|SentimentsConfig)\b/gu,
    ],
    [
      "crates/labcolors-wasm/src/config_dto.rs",
      /\b(?:SentimentCategoryDto|SentimentsDto)\b|LadderSourceDto::Sentiment/gu,
    ],
    [
      "crates/labcolors-wasm/src/lib.rs",
      /\{ kind: "sentiment"|readonly sentiments\s*:/gu,
    ],
  ];

  for (const [path, forbidden] of surfaces) {
    assert.doesNotMatch(
      readFileSync(join(ROOT, path), "utf8"),
      forbidden,
      `${path}: retired sentiment-specific API must not resurface`,
    );
  }

  const shippedSourceFiles = [...productionRustFiles(), ...productionPackageFiles()];
  const shippedSourcePaths = shippedSourceFiles.map((path) => relative(ROOT, path));
  for (const required of [
    "crates/labcolors-core/src/semantic.rs",
    "crates/labcolors-wasm/src/projection.rs",
    "crates/labcolors-ffi/src/lib.rs",
    "crates/labcolors-conformance/src/lib.rs",
    "packages/colors/index.js",
    "packages/colors/index.d.ts",
  ]) {
    assert.ok(shippedSourcePaths.includes(required), `shipped-source scan omitted ${required}`);
  }
  assert.ok(
    shippedSourcePaths.every((path) =>
      path.endsWith(".rs") || !/(?:^|\/)pkg(?:\/|$)/u.test(path),
    ),
    "shipped-source scan admitted a generated npm binding",
  );
  for (const path of shippedSourceFiles) {
    const source = readFileSync(path, "utf8");
    const code = path.endsWith(".rs") ? maskRustNonCode(source) : source;
    assert.doesNotMatch(
      code,
      RETIRED_SENTIMENT_SYMBOL,
      `${relative(ROOT, path)}: retired sentiment symbol resurfaced in shipped source`,
    );
  }

  const maskedNegativeFixtures = [
    "// SentimentCurve and LadderSource::Sentiment are retired",
    "/* nested /* SentimentsConfig */ comment */",
    'const OLD: &str = "SentimentCurve 😀 { };";',
    'const RAW: &str = r#"LadderSource::Sentiment"#;',
    "const LETTER: char = 'S';",
    "struct CurrentProductionType;",
  ].join("\n");
  assert.doesNotMatch(
    maskRustNonCode(maskedNegativeFixtures),
    RETIRED_SENTIMENT_SYMBOL,
    "comments and literal negative fixtures must not poison the source scan",
  );
  const retiredTestType = [
    "#[cfg(test)]",
    "struct SentimentCurve;",
  ].join("\n");
  assert.match(
    maskRustNonCode(retiredTestType),
    RETIRED_SENTIMENT_SYMBOL,
    "retired model code is forbidden even when cfg(test)-gated",
  );

  const documentation = [
    ...publicClaimFiles().filter((path) => /(?:\.md|\.d\.ts)$/u.test(path)),
    ...claimFiles(join(ROOT, "docs"), [], /\.md$/u),
    ...claimFiles(join(ROOT, "reference"), [], /\.md$/u),
  ].filter((path, index, all) => all.indexOf(path) === index);
  for (const path of documentation) {
    assert.doesNotMatch(
      readFileSync(path, "utf8"),
      RETIRED_SENTIMENT_MODEL,
      `${relative(ROOT, path)}: retired sentiment model must not survive as live documentation`,
    );
  }

  const canonicalConfigSurfaces = [
    "crates/labcolors-wasm/tests/data/labui.config.json",
    "crates/labcolors-wasm/tests/data/labui.config.prod.json",
    "crates/labcolors-wasm/tests/chain_invariants.rs",
    "crates/labcolors-wasm/tests/wasm_parity.rs",
    "scripts/verify-package-release.mjs",
  ];
  for (const path of canonicalConfigSurfaces) {
    assert.doesNotMatch(
      readFileSync(join(ROOT, path), "utf8"),
      /["']sentiments["']|["']kind["']\s*:\s*["']sentiment["']/iu,
      `${path}: canonical configs must not preserve the retired wire schema`,
    );
  }
});

test("the retired Lab UI accent catalogue stays absent from generic Core", () => {
  assert.equal(
    existsSync(join(ROOT, "crates/labcolors-core/src/accent.rs")),
    false,
    "the closed Lab UI catalogue must not survive as a self-testing fixture",
  );
  const lib = readFileSync(join(ROOT, "crates/labcolors-core/src/lib.rs"), "utf8");
  assert.doesNotMatch(lib, /\bmod accent\s*;/u);

  for (const path of [
    "crates/labcolors-core/src/config.rs",
    "crates/labcolors-core/src/semantic.rs",
  ]) {
    assert.doesNotMatch(
      readFileSync(join(ROOT, path), "utf8"),
      /crate::accent::/u,
      `${path}: generic colour math must not depend on the Lab UI test catalogue`,
    );
  }
});
