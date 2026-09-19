import test from "node:test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const root = fileURLToPath(new URL("../../../", import.meta.url));
const script = fileURLToPath(new URL("../../../scripts/test_certificate_source_build.py", import.meta.url));

test("Core source descriptor survives real Git and Cargo cache histories", { timeout: 240_000 }, async () => {
  try {
    // Python владеет deadline и cleanup всей command group; не обрываем его finally.
    await run("python3", [script], { cwd: root, maxBuffer: 2 * 1024 * 1024 });
  } catch (error) {
    error.message += `\n${error.stdout ?? ""}\n${error.stderr ?? ""}`;
    throw error;
  }
});
