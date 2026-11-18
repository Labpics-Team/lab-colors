#!/usr/bin/env bash
# The crate lockfile selects the CLI; GitHub's signed immutable release binds its bytes.
set -euo pipefail
source_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
version="$(python3 - "$source_root/Cargo.lock" <<'PY'
import re
import sys
import tomllib
from pathlib import Path

packages = [p for p in tomllib.loads(Path(sys.argv[1]).read_text())["package"]
            if p["name"] == "wasm-bindgen"]
if (len(packages) != 1 or
        packages[0].get("source") != "registry+https://github.com/rust-lang/crates.io-index" or
        re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", packages[0]["version"]) is None):
    raise SystemExit("Cargo.lock must select exactly one crates.io wasm-bindgen release")
print(packages[0]["version"])
PY
)"
work="$(mktemp -d "${RUNNER_TEMP:?}/wasm-bindgen.XXXXXX")"
trap 'rm -rf -- "$work"' EXIT
asset="wasm-bindgen-$version-x86_64-unknown-linux-musl.tar.gz"
curl --fail-with-body --silent --show-error --location --retry 3 --retry-all-errors \
  "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$version/$asset" \
  --output "$work/$asset"
# No extraction or execution is allowed before the signed asset check succeeds.
gh release verify-asset "$version" "$work/$asset" \
  --repo github.com/wasm-bindgen/wasm-bindgen
mkdir "$work/unpacked" "$work/cli"
tar --extract --gzip --strip-components=1 --file="$work/$asset" --directory="$work/unpacked"
mv -- "$work/unpacked/wasm-bindgen" "$work/cli/wasm-bindgen"
mv -- "$work/unpacked/wasm-bindgen-test-runner" "$work/cli/wasm-bindgen-test-runner"
test "$("$work/cli/wasm-bindgen" --version)" = "wasm-bindgen $version"
final_root="${WASM_PACK_CACHE:?}/wasm-bindgen-cargo-install-$version"
mkdir -p -- "$WASM_PACK_CACHE"
rm -rf -- "$final_root"
mv -- "$work/cli" "$final_root"
printf '%s\n' "$final_root" >> "${GITHUB_PATH:?}"
