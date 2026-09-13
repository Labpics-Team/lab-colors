import { mkdtemp, open, rename, rm } from "node:fs/promises";
import { basename, dirname, join } from "node:path";

const GENERATED_FILE_MODE = 0o644;

// Каждое освобождение независимо: вторичный отказ не прерывает оставшуюся
// очистку и не уничтожает первичную причину. Даже throw undefined есть отказ.
async function withCleanup(operation, releases) {
  const failures = [];
  try {
    await operation();
  } catch (error) {
    failures.push(error);
  }
  for (const release of releases) {
    try {
      await release();
    } catch (error) {
      failures.push(error);
    }
  }
  if (failures.length === 1) throw failures[0];
  if (failures.length > 1) {
    throw new AggregateError(failures, "Generated file operation and cleanup failed", {
      cause: failures[0],
    });
  }
}

/**
 * Flush a directory so a rename that landed in it survives a crash. On POSIX a
 * rename only becomes durable when the destination directory is fsynced after
 * the rename; fsyncing the file alone does not flush the new directory entry.
 * Win32 cannot open a directory handle for flushing in Node, so it keeps the
 * pre-rename file fsync as its durability floor (the release worker states the
 * same platform contract).
 */
export async function fsyncDirectory(directory) {
  if (process.platform === "win32") return;
  const handle = await open(directory, "r");
  await withCleanup(() => handle.sync(), [() => handle.close()]);
}

/**
 * Replace one generated file without ever opening a caller-controlled temporary path.
 * `options.fsyncDirectory` is a narrow seam for observing and faulting the
 * post-rename directory flush; the write and rename always run for real.
 * A sole failure is rethrown unchanged; combined operation/cleanup failures
 * produce an AggregateError in observation order, with the first as its cause.
 * A failure after rename does not promise that the old destination was restored.
 */
export async function atomicWriteGeneratedFile(path, bytes, options = {}) {
  const flushDirectory = options.fsyncDirectory ?? fsyncDirectory;
  const temporaryDirectory = await mkdtemp(
    join(dirname(path), `.${basename(path)}.tmp-`),
  );
  const temporary = join(temporaryDirectory, "content");
  let handle;
  await withCleanup(async () => {
    handle = await open(temporary, "wx", GENERATED_FILE_MODE);
    await handle.chmod(GENERATED_FILE_MODE);
    await handle.writeFile(bytes);
    await handle.sync();
    // Передаём владение единственной попытке close до rename. Не повторяем
    // её из cleanup, если завершившееся закрытие само сообщило ошибку.
    const completed = handle;
    handle = undefined;
    await completed.close();
    await rename(temporary, path);
    // Fail closed: a directory-flush error rejects the write even though the
    // rename itself succeeded, because the new entry is not yet durable.
    await flushDirectory(dirname(path));
  }, [
    () => handle?.close(),
    // The unpredictable directory is created mode 0700 by mkdtemp. Removing
    // that directory path cannot follow a pre-planted temporary-file symlink.
    () => rm(temporaryDirectory, { recursive: true, force: true }),
  ]);
}
