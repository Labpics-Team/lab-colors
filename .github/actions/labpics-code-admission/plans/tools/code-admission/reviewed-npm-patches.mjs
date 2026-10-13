import { createNpmPatchQualifier } from './npm-patch.mjs';

export const BRACES = Object.freeze({
  "name": "braces",
  "version": "3.0.3",
  "patchHash": "wf4kvghcw4swvzhqehyrzkwgvq",
  "patchSha256": "4ab30ccf53ccdaa35119e45e65f29c948d87b665c122a665154abb4abea6ecd9",
  "inventoryFiles": 11,
  "inventorySha256": "47934cd63f702f643317de3b46a712a42afcdfd6aee24147507b8c1812da893d",
  "rules": [
    "CVE-2026-93687",
    "GHSA-vfj7-8cjw-p6xm"
  ],
  "packageManager": "pnpm@9.15.0",
  "patchPath": "patches/braces@3.0.3.patch",
  "archiveSha256": "1cd18e862c8640b4568b1425a7df4ee030ff201d45b2da8f9f222d2987494ffc",
  "integrity": "sha512-yQbXgO/OSZVD2IsiLlro+7Hf6Q18EJrKSEsdoMzKePKXct3gvD8oLcOQdIzGupr5Fj+EDe8gO/lxc1BzfMpxvA==",
  "advisorySha256": "bc24e4bafeb80afb949b0343721ce288486f93251fa8d221060b58cdad875e31",
  "reviewedAt": "2026-10-05T00:00:00Z",
  "source": "Labpics-Team/robots@59e12c63d819c75fcaab6fafea2ce1f8374cf16a",
  "upstream": "https://github.com/micromatch/braces/issues/70",
  "dependencies": {
    "fill-range": "7.1.1"
  },
  "installDependencies": [
    {
      "name": "fill-range",
      "version": "7.1.1",
      "integrity": "sha512-YsGpe3WHLK8ZYi4tWDg2Jy3ebRz2rXowDxnld4bkQB00cc/1Zw9AWnC0i9ztDJitivtQvaI9KaLyKrc+hBW0yg==",
      "archiveSha256": "1a6c3a3446cb1d693cf6b62785e2f6d8f9596305aff474dd2b31b2d8d427593c",
      "dependencies": {
        "to-regex-range": "5.0.1"
      }
    },
    {
      "name": "to-regex-range",
      "version": "5.0.1",
      "integrity": "sha512-65P7iz6X5yEr1cwcgvQxbbIw7Uk3gOy5dIdtZ4rDveLqhrdJP+Li/Hx6tyK0NEb+2GCyneCMJiGqrADCSNk8sQ==",
      "archiveSha256": "a63a255ec20a935c3a3334a6645f9245db36e2319ddf1eb6bc0eab2bfe5b42d7",
      "dependencies": {
        "is-number": "7.0.0"
      }
    },
    {
      "name": "is-number",
      "version": "7.0.0",
      "integrity": "sha512-41Cifkg6e8TylSpdtTpeLVMqvSBEVzTttHvERD741+pnZ8ANv0004MRL43QKPDlK9cGvNp6NZWZUBlbGXYxxng==",
      "archiveSha256": "7b75c1057198cf97696909a9bee176c9c5e9bcb5b03bf3ecef2f484defadd51e"
    }
  ]
});

export const HTTP_CACHE = Object.freeze({
  "name": "http-cache-semantics",
  "version": "4.2.0",
  "patchHash": "dp66pakf7igbpbcy43a57a7yrm",
  "patchSha256": "37c4236e0bcd26d671ae12c350b18e50d510a9d12d7d8d74a5e873726a4d4e05",
  "inventoryFiles": 4,
  "inventorySha256": "fe91e1c4706e1de0dd270b54ccc98791c43aa04339a22639e0ddc7a385bc0938",
  "rules": [
    "CVE-2026-93748",
    "GHSA-ch52-4w7c-c8xp"
  ],
  "packageManager": "pnpm@9.15.0",
  "patchPath": "patches/http-cache-semantics@4.2.0.patch",
  "archiveSha256": "f57454db8ab2d06a4baabbfb7c70ad4181a00b7ee0efebcdffa9ba313bd80eb7",
  "integrity": "sha512-dTxcvPXqPvXBQpq5dUr6mEMJX4oIEFv6bwom3FDwKRDsuIjjJGANqhBuoAn9c1RQJIdAKav33ED65E2ys+87QQ==",
  "advisorySha256": "4baedcea200da55ee2f66a4e3c700a03996039e7bcf3af4b75dee34e8ed1088c",
  "reviewedAt": "2026-10-05T00:00:00Z",
  "source": "Labpics-Team/robots@59e12c63d819c75fcaab6fafea2ce1f8374cf16a",
  "upstream": "https://github.com/kornelski/http-cache-semantics/issues/56",
  "distinctPackages": {
    "@types/http-cache-semantics@4.2.0": {
      "package": {
        "resolution": {
          "integrity": "sha512-L3LgimLHXtGkWikKnsPg0/VFx9OGZaC+eN1u4r+OB1XRqH3meBIAVC2zr1WdMH+RHmnRkqliQAOHNJ/E0j/e0Q=="
        }
      },
      "snapshot": {}
    }
  }
});

const reviewed = [BRACES, HTTP_CACHE].map(createNpmPatchQualifier);
export function qualifyReviewedNpmFindings(root, findings, parent) {
  return reviewed.flatMap(qualification => qualification.qualifyFindings(root, findings, parent));
}
