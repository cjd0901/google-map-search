import { cp, copyFile, mkdir, rm, stat } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runtimeDir = path.join(projectRoot, "runtime");
const playwrightSource = path.join(projectRoot, "node_modules", "playwright-core");
const playwrightTarget = path.join(runtimeDir, "node_modules", "playwright-core");
const headlessShellTarget = path.join(runtimeDir, "browsers", "chromium-headless-shell");
const nodeBinaryName = process.platform === "win32" ? "node.exe" : "node";

async function run(command, args, options = {}) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: "inherit",
      ...options,
    });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`命令执行失败（code=${code}, signal=${signal || "none"}）`));
    });
  });
}

async function ensureMacHeadlessShell() {
  if (process.platform !== "darwin") return null;

  const browsers = JSON.parse(
    await import("node:fs/promises").then(({ readFile }) =>
      readFile(path.join(playwrightSource, "browsers.json"), "utf8"),
    ),
  );
  const descriptor = browsers.browsers.find(
    (browser) => browser.name === "chromium-headless-shell",
  );
  if (!descriptor) throw new Error("playwright-core 未声明 chromium-headless-shell");

  const browserPath = path.join(
    playwrightSource,
    ".local-browsers",
    `chromium_headless_shell-${descriptor.revision}`,
  );
  const browser = await stat(browserPath).catch(() => null);
  if (browser?.isDirectory()) return browserPath;

  console.log("首次构建 macOS 版本，正在下载无程序坞图标的后台浏览器…");
  await run(
    process.execPath,
    [path.join(playwrightSource, "cli.js"), "install", "chromium-headless-shell"],
    {
      env: {
        ...process.env,
        PLAYWRIGHT_BROWSERS_PATH: "0",
      },
    },
  );
  return browserPath;
}

async function assertFile(filePath, description) {
  const file = await stat(filePath).catch(() => null);
  if (!file?.isFile()) {
    throw new Error(`${description}不存在：${filePath}`);
  }
}

async function prepareRuntime() {
  await assertFile(process.execPath, "Node.js 运行时");
  await assertFile(path.join(playwrightSource, "package.json"), "playwright-core");
  const headlessShellSource = await ensureMacHeadlessShell();
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
    filter: (source) => path.basename(source) !== ".local-browsers",
  });

  await rm(headlessShellTarget, { recursive: true, force: true });
  if (headlessShellSource) {
    await mkdir(path.dirname(headlessShellTarget), { recursive: true });
    await cp(headlessShellSource, headlessShellTarget, {
      recursive: true,
      dereference: true,
    });
  }

  const nodeLicense = path.join(projectRoot, "licenses", "NODE-LICENSE.txt");
  await assertFile(nodeLicense, "Node.js 许可证文件");
  await mkdir(path.join(runtimeDir, "licenses"), { recursive: true });
  await copyFile(nodeLicense, path.join(runtimeDir, "licenses", "NODE-LICENSE.txt"));
}

await prepareRuntime();
