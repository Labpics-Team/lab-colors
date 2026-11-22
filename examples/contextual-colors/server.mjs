import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(fileURLToPath(import.meta.url));
const mime = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".wasm": "application/wasm", ".json": "application/json" };
const port = Number(process.env.PORT ?? 4173);
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
    const file = resolve(root, "." + (pathname === "/" ? "/index.html" : pathname));
    if (!file.startsWith(root + sep)) { response.writeHead(403); response.end(); return; }
    const body = await readFile(file);
    response.writeHead(200, { "content-type": mime[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" });
    response.end(body);
  } catch { response.writeHead(404); response.end("Not found"); }
});
server.listen(port, "127.0.0.1", () => process.stdout.write(`http://127.0.0.1:${server.address().port}\n`));
for (const signal of ["SIGTERM", "SIGINT"]) process.once(signal, () => server.close());
