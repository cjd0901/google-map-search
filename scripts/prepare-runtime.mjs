import { cp, copyFile, mkdir, rm, stat } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runtimeDir = path.join(projectRoot, "runtime");
const playwrightSource = path.join(projectRoot, "node_modules", "playwright-core");
const playwrightTarget = path.join(runtimeDir, "node_modules", "playwright-core");
const nodeBinaryName = process.platform === "win32" ? "node.exe" : "node";

async function assertFile(filePath, description) {
  const file = await stat(filePath).catch(() => null);
  if (!file?.isFile()) {
    throw new Error(`${description}不存在：${filePath}`);
  }
}

async function prepareRuntime() {
  await assertFile(process.execPath, "Node.js 运行时");
  await assertFile(path.join(playwrightSource, "package.json"), "playwright-core");
  await mkdir(runtimeDir, { recursive: true });

  await copyFile(process.execPath, path.join(runtimeDir, nodeBinaryName));
  if (process.platform !== "win32") {
    await import("node:fs/promises").then(({ chmod }) =>
      chmod(path.join(runtimeDir, nodeBinaryName), 0o755),
    );
  }
  await copyFile(
    path.join(projectRoot, "crawler", "google_maps.mjs"),
    path.join(runtimeDir, "google_maps.mjs"),
  );

  await rm(playwrightTarget, { recursive: true, force: true });
  await mkdir(path.dirname(playwrightTarget), { recursive: true });
  await cp(playwrightSource, playwrightTarget, {
    recursive: true,
    dereference: true,
  });

  const nodeLicense = path.join(projectRoot, "licenses", "NODE-LICENSE.txt");
  await assertFile(nodeLicense, "Node.js 许可证文件");
  await mkdir(path.join(runtimeDir, "licenses"), { recursive: true });
  await copyFile(nodeLicense, path.join(runtimeDir, "licenses", "NODE-LICENSE.txt"));
}

await prepareRuntime();
