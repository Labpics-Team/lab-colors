export const RELEASE_EVIDENCE_INVENTORY = Object.freeze({
  wcag22: Object.freeze({
    files: Object.freeze([
      "wcag22-srgb8-v1.json",
      "wcag22-srgb8-q55-v1.bin",
      "wcag22-srgb8-q55-proof-v1.json",
    ]),
  }),
  pointSupportReferenceSurplus: Object.freeze({
    files: Object.freeze([
      "point-support-reference-surplus-q55-bps-proof-v1.json",
    ]),
  }),
});

export const WCAG22_EVIDENCE_FILES = RELEASE_EVIDENCE_INVENTORY.wcag22.files;
export const POINT_SUPPORT_EVIDENCE_FILES =
  RELEASE_EVIDENCE_INVENTORY.pointSupportReferenceSurplus.files;
export const NUMERICAL_EVIDENCE_FILES = Object.freeze(
  Object.values(RELEASE_EVIDENCE_INVENTORY).flatMap(({ files }) => files),
);
export const PACKED_NUMERICAL_EVIDENCE_PATHS = Object.freeze(
  NUMERICAL_EVIDENCE_FILES.map((file) => `evidence/${file}`),
);

export function assertPackageEvidenceInventory(packageFiles) {
  if (!Array.isArray(packageFiles)) {
    throw new Error("package.json files must be an array");
  }
  const declared = packageFiles
    .filter((path) => typeof path === "string" && path.startsWith("evidence/"))
    .sort();
  const expected = [...PACKED_NUMERICAL_EVIDENCE_PATHS].sort();
  if (
    declared.length !== expected.length ||
    declared.some((path, index) => path !== expected[index])
  ) {
    throw new Error(
      `package.json evidence inventory ${JSON.stringify(declared)} differs from ` +
        `the canonical release inventory ${JSON.stringify(expected)}`,
    );
  }
}
