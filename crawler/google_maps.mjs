import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { chromium } from "playwright-core";

// The macOS package carries Playwright's standalone headless shell. Unlike
// Google Chrome.app, this executable does not register another Dock tile.
function bundledHeadlessShellPath() {
  if (process.platform !== "darwin") return null;
  const architecture = process.arch === "arm64" ? "arm64" : "x64";
  const scriptDir = path.dirname(fileURLToPath(import.meta.url));
  const relativePath = path.join(
    "browsers",
    "chromium-headless-shell",
    `chrome-headless-shell-mac-${architecture}`,
    "chrome-headless-shell",
  );
  const candidates = [
    path.join(scriptDir, relativePath),
    path.join(scriptDir, "..", "runtime", relativePath),
  ];
  return candidates.find((candidate) => existsSync(candidate)) || null;
}

function emit(type, payload = {}) {
  process.stdout.write(`${JSON.stringify({ type, ...payload })}\n`);
}

function clean(value = "") {
  return String(value).replace(/\s+/g, " ").trim();
}

function stripLabel(value = "") {
  return clean(value).replace(
    /^(地址|电话|网站|Address|Phone|Website|Located in|位于)\s*[:：]?\s*/i,
    "",
  );
}

export function parseRating(value = "") {
  const match = clean(value).match(/(\d+(?:[.,]\d+)?)\s*(?:星|stars?)/i);
  return match ? Number(match[1].replace(",", ".")) : null;
}

export function parseReviewCount(value = "") {
  const normalized = clean(value);
  const match =
    normalized.match(/([\d,，.\s]+)\s*(?:条)?(?:评价|评论|reviews?)/i) ||
    normalized.match(/\(([\d,，.\s]+)\)/);
  if (!match) return null;
  const count = Number(match[1].replace(/[,，.\s]/g, ""));
  return Number.isFinite(count) ? count : null;
}

async function launchBrowser(headless) {
  const userDataDir = path.join(os.tmpdir(), "yingfeng-data-browser-profile");
  const common = {
    headless,
    viewport: { width: 1440, height: 900 },
    locale: "zh-CN",
  };
  const candidates = [];
  if (process.env.GOOGLE_MAPS_BROWSER_EXECUTABLE) {
    candidates.push({ executablePath: process.env.GOOGLE_MAPS_BROWSER_EXECUTABLE });
  }
  const headlessShell = bundledHeadlessShellPath();
  if (headless && headlessShell) {
    candidates.push({ executablePath: headlessShell });
  }
  if (!headless || !headlessShell) {
    candidates.push({ channel: "msedge" }, { channel: "chrome" });
  }

  let lastError;
  for (const candidate of candidates) {
    try {
      const context = await chromium.launchPersistentContext(userDataDir, {
        ...common,
        ...candidate,
      });
      emit("diagnostic", {
        message: `browser=${candidate.executablePath || candidate.channel || "playwright"}`,
      });
      return context;
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(
    `未找到可用的后台浏览器或 Edge/Chrome。请重新构建应用、安装浏览器，或设置 GOOGLE_MAPS_BROWSER_EXECUTABLE。${lastError ? ` ${lastError.message}` : ""}`,
  );
}

async function acceptConsent(page) {
  const labels = [
    /Accept all/i,
    /I agree/i,
    /全部接受/,
    /接受全部/,
    /同意/,
  ];
  for (const label of labels) {
    const button = page.getByRole("button", { name: label }).first();
    if (await button.isVisible().catch(() => false)) {
      await button.click().catch(() => {});
      await page.waitForTimeout(800);
      return;
    }
  }
}

async function isBlocked(page) {
  const url = page.url().toLowerCase();
  if (url.includes("/sorry/") || url.includes("recaptcha")) return true;
  const text = clean(await page.locator("body").innerText().catch(() => ""));
  return /unusual traffic|verify you are human|验证您是真人|异常流量|captcha/i.test(text);
}

async function waitForManualVerification(page) {
  if (!(await isBlocked(page))) return;
  emit("blocked", {
    message: "Google 要求人工验证，请在浏览器中完成验证；任务会自动继续。",
  });
  const deadline = Date.now() + 10 * 60 * 1000;
  while (Date.now() < deadline) {
    await page.waitForTimeout(3000);
    if (!(await isBlocked(page))) {
      emit("status", { message: "验证已完成，继续采集" });
      return;
    }
  }
  throw new Error("等待人工验证超时");
}

async function collectPlaceUrls(page, maxResults) {
  const result = [];
  const seen = new Set();
  const addVisibleLinks = async () => {
    const links = await page
      .locator('a[href*="/maps/place/"]')
      .evaluateAll((elements) =>
        elements.map((element) => {
          let card = element.closest(".Nv2PK");
          if (!card) {
            let candidate = element.parentElement;
            for (let depth = 0; candidate && depth < 6; depth += 1) {
              const hasRating = Array.from(candidate.querySelectorAll("[aria-label]")).some(
                (node) => /\d+(?:[.,]\d+)?\s*(?:星|stars?)/i.test(node.getAttribute("aria-label") || ""),
              );
              if (hasRating && (candidate.textContent || "").length < 1200) {
                card = candidate;
                break;
              }
              candidate = candidate.parentElement;
            }
          }
          const labels = card
            ? Array.from(card.querySelectorAll("[aria-label]")).map(
                (node) => node.getAttribute("aria-label") || "",
              )
            : [];
          const ratingLabel =
            labels.find((label) => /\d+(?:[.,]\d+)?\s*(?:星|stars?)/i.test(label)) || "";
          const reviewLabel =
            labels.find((label) => /[\d,，.\s]+\s*(?:条)?(?:评价|评论|reviews?)/i.test(label)) ||
            ratingLabel ||
            card?.textContent ||
            "";
          return { href: element.href, ratingLabel, reviewLabel };
        }),
      )
      .catch(() => []);
    for (const link of links) {
      const key = link.href.split("&")[0];
      if (!seen.has(key)) {
        seen.add(key);
        result.push({
          mapsUrl: link.href,
          rating: parseRating(link.ratingLabel),
          reviewCount: parseReviewCount(link.reviewLabel),
        });
      }
      if (result.length >= maxResults) break;
    }
  };

  const feed = page.locator('div[role="feed"]').first();
  if (!(await feed.count())) {
    if (page.url().includes("/maps/place/")) {
      result.push({ mapsUrl: page.url(), rating: null, reviewCount: null });
    }
    await addVisibleLinks();
    return result.slice(0, maxResults);
  }

  let stagnantRounds = 0;
  let previousCount = 0;
  while (result.length < maxResults && stagnantRounds < 6) {
    await addVisibleLinks();
    emit("progress", {
      discovered: Math.min(result.length, maxResults),
      message: `已发现 ${Math.min(result.length, maxResults)} 家商户`,
    });
    if (result.length === previousCount) stagnantRounds += 1;
    else stagnantRounds = 0;
    previousCount = result.length;
    await feed.evaluate((element) => element.scrollTo(0, element.scrollHeight));
    await page.waitForTimeout(1300);
    const bodyText = await feed.innerText().catch(() => "");
    if (/You've reached the end of the list|已到达列表末尾|没有更多结果/i.test(bodyText)) break;
  }
  return result.slice(0, maxResults);
}

async function extractBusiness(page, mapsUrl) {
  await page.locator("h1").first().waitFor({ state: "visible", timeout: 12_000 });
  await page
    .waitForFunction(
      () =>
        document.querySelector("[data-item-id]") ||
        document.querySelector(
          'button[jsaction*="category"], button[jsaction*="pane.rating.category"]',
        ),
      undefined,
      { timeout: 2500 },
    )
    .catch(() => {});
  // Read the optional fields in one browser round trip. Locator methods inherit
  // the 15-second default timeout, which previously added that delay for every
  // missing category, rating, or review element.
  const snapshot = await page.evaluate(() => {
    const text = (element) => element?.textContent || "";
    const categoryNode = document.querySelector(
      'button[jsaction*="category"], button[jsaction*="pane.rating.category"]',
    );
    const itemNodes = Array.from(document.querySelectorAll("[data-item-id]")).map(
      (element) => ({
        id: element.getAttribute("data-item-id") || "",
        text: element.getAttribute("aria-label") || text(element),
        href: element instanceof HTMLAnchorElement ? element.href : "",
      }),
    );
    const ariaLabels = Array.from(document.querySelectorAll("[aria-label]"))
      .map((element) => element.getAttribute("aria-label") || "")
      .filter(Boolean);
    return {
      name: text(document.querySelector("h1")),
      category: text(categoryNode),
      itemNodes,
      ratingLabel:
        ariaLabels.find((label) => /\d+(?:[.,]\d+)?\s*(?:星|stars?)/i.test(label)) || "",
      reviewLabel:
        ariaLabels.find((label) => /[\d,，.\s]+\s*(?:条)?(?:评价|评论|reviews?)/i.test(label)) ||
        "",
    };
  });

  const addressNode = snapshot.itemNodes.find((item) => item.id === "address");
  const phoneNode = snapshot.itemNodes.find((item) => item.id.startsWith("phone:tel:"));
  const websiteNode = snapshot.itemNodes.find((item) => item.id === "authority");

  return {
    name: clean(snapshot.name),
    category: clean(snapshot.category),
    address: stripLabel(addressNode?.text || ""),
    phone: stripLabel(phoneNode?.text || phoneNode?.id.replace("phone:tel:", "") || ""),
    website: websiteNode?.href || "",
    mapsUrl,
    rating: parseRating(snapshot.ratingLabel),
    reviewCount: parseReviewCount(snapshot.reviewLabel),
  };
}

async function main() {
  const request = JSON.parse(process.argv[2] || "{}");
  emit("diagnostic", {
    message: `node=${process.version} platform=${process.platform} arch=${process.arch} script=${import.meta.url}`,
  });
  const keyword = clean(request.keyword);
  const location = clean(request.location);
  const maxResults = Math.max(1, Math.min(Number(request.maxResults) || 20, 200));
  const language = clean(request.language || "zh-CN");
  if (!keyword || !location) throw new Error("关键词和地区不能为空");

  emit("status", { message: "正在启动 Edge/Chrome" });
  const context = await launchBrowser(request.headless !== false);
  const page = context.pages()[0] || (await context.newPage());
  page.setDefaultTimeout(15_000);
  let shuttingDown = false;
  const close = async () => {
    if (shuttingDown) return;
    shuttingDown = true;
    await context.close().catch(() => {});
  };
  process.once("SIGTERM", close);
  process.once("SIGINT", close);

  try {
    const query = `${keyword} ${location}`;
    const url = `https://www.google.com/maps/search/${encodeURIComponent(query)}?hl=${encodeURIComponent(language)}`;
    emit("status", { message: `正在搜索：${query}` });
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 45_000 });
    await acceptConsent(page);
    await waitForManualVerification(page);
    await page.waitForTimeout(1800);

    const places = await collectPlaceUrls(page, maxResults);
    if (!places.length) {
      throw new Error("没有找到商家。请检查关键词、地区或浏览器中的提示。");
    }
    emit("progress", {
      discovered: places.length,
      message: `发现 ${places.length} 家商户，开始读取详情`,
    });

    let completed = 0;
    for (const place of places) {
      const { mapsUrl } = place;
      const startedAt = performance.now();
      await page.goto(mapsUrl, { waitUntil: "domcontentloaded", timeout: 35_000 });
      await waitForManualVerification(page);
      const navigatedAt = performance.now();
      try {
        const data = await extractBusiness(page, mapsUrl);
        data.rating ??= place.rating;
        data.reviewCount ??= place.reviewCount;
        if (data.name) emit("business", { data });
        else emit("error", { message: "跳过一个无法识别名称的商家" });
        emit("diagnostic", {
          message: `detail=${completed + 1}/${places.length} navigation_ms=${Math.round(navigatedAt - startedAt)} extraction_ms=${Math.round(performance.now() - navigatedAt)} total_ms=${Math.round(performance.now() - startedAt)} category=${Boolean(data.category)} rating=${data.rating ?? "missing"} reviews=${data.reviewCount ?? "missing"}`,
        });
      } catch (error) {
        emit("error", { message: `读取商家详情失败：${error.message}` });
      }
      completed += 1;
      emit("progress", {
        discovered: places.length,
        message: `已读取 ${completed}/${places.length} 家商户详情`,
      });
    }
    emit("done", { discovered: places.length });
  } finally {
    await close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    emit("error", { message: error?.message || String(error) });
    process.exitCode = 1;
  });
}
