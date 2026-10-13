import { createNpmPatchQualifier, NPM_TOOLS } from './npm-patch.mjs';

export const FORGE = Object.freeze({
  name: 'node-forge', inventoryFiles: 59, version: '1.4.0', packageManager: 'pnpm@9.15.0',
  rules: Object.freeze(['CVE-2026-85393', 'GHSA-86w9-cpqp-85rv']),
  patchPath: 'patches/node-forge@1.4.0.patch',
  patchSha256: '0127c3750e7e6cf3cdb549891e412518d5eb7c604338748808144a181bcccbe3',
  patchHash: 'vio2umvfvqw3w3ciciybqypooa',
  archiveSha256: 'bf9d7ca0d774235354697bd4b5e642af6505e7ce2066762c3b855138cf870820',
  integrity: 'sha512-LarFH0+6VfriEhqMMcLX2F7SwSXeWwnEAJEsYm5QKWchiVYVvJyV9v7UDvUv+w5HO23ZpQTXDv/GxdDdMyOuoQ==',
  inventorySha256: 'f117137c8981e292411be0043ca50e31bf79f48abcc96b9a76b7d19f0e461dff',
  advisorySha256: '2358ffc585641a1508c2c3575f3679a88f5a0fcc57e743e9b30f977c37a0042b',
  reviewedAt: '2026-10-02T00:00:00Z',
  source: 'Labpics-Team/robots@f5fa33509be2100edea5fe6cba5f785f8c647d2d',
  upstream: 'https://github.com/digitalbazaar/forge/pull/1152/commits/ceba34402e329f0365134f23fe19898756527d65',
});
export const FORGE_TOOLS = NPM_TOOLS;
const forge = createNpmPatchQualifier(FORGE);
export const parseForgeYaml = forge.parsePatchYaml;
export const forgeLockClosure = forge.lockClosure;
export const forgeSubject = forge.subject;
export const forgeAdvisory = forge.advisory;
export const forgeInventory = forge.inventory;
export const qualifyForgeFindings = forge.qualifyFindings;
