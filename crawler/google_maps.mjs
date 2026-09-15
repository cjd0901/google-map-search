import os from "node:os";
import path from "node:path";
import process from "node:process";
import { chromium } from "playwright-core";

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

function parseNumber(value = "") {
  const normalized = value.replace(/[,，\s]/g, "");
  const match = normalized.match(/\d+(?:\.\d+)?/);
  return match ? Number(match[0]) : null;
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
  candidates.push({ channel: "msedge" }, { channel: "chrome" });

  let lastError;
  for (const candidate of candidates) {
    try {
      return await chromium.launchPersistentContext(userDataDir, {
        ...common,
        ...candidate,
      });
    } catch (error) {
      lastError = error;
    }
  }
  throw new Error(
    `未找到可用的 Edge/Chrome。请安装浏览器，或设置 GOOGLE_MAPS_BROWSER_EXECUTABLE。${lastError ? ` ${lastError.message}` : ""}`,
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
      .evaluateAll((elements) => elements.map((element) => element.href))
      .catch(() => []);
    for (const href of links) {
      const key = href.split("&")[0];
      if (!seen.has(key)) {
        seen.add(key);
        result.push(href);
      }
      if (result.length >= maxResults) break;
    }
  };

  const feed = page.locator('div[role="feed"]').first();
  if (!(await feed.count())) {
    if (page.url().includes("/maps/place/")) result.push(page.url());
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
  const name = clean(await page.locator("h1").first().innerText());
  const itemNodes = await page
    .locator("[data-item-id]")
    .evaluateAll((elements) =>
      elements.map((element) => ({
        id: element.getAttribute("data-item-id") || "",
        text: element.getAttribute("aria-label") || element.textContent || "",
        href: element instanceof HTMLAnchorElement ? element.href : "",
      })),
    )
    .catch(() => []);

  const addressNode = itemNodes.find((item) => item.id === "address");
  const phoneNode = itemNodes.find((item) => item.id.startsWith("phone:tel:"));
  const websiteNode = itemNodes.find((item) => item.id === "authority");
  const category = clean(
    await page
      .locator('button[jsaction*="category"], button[jsaction*="pane.rating.category"]')
      .first()
      .innerText()
      .catch(() => ""),
  );
  const ratingLabel = await page
    .locator('div[role="img"][aria-label*="星"], div[role="img"][aria-label*="star"]')
    .first()
    .getAttribute("aria-label")
    .catch(() => "");
  const reviewLabel = await page
    .locator('button[aria-label*="评价"], button[aria-label*="review"]')
    .first()
    .getAttribute("aria-label")
    .catch(() => "");

  return {
    name,
    category,
    address: stripLabel(addressNode?.text || ""),
    phone: stripLabel(phoneNode?.text || phoneNode?.id.replace("phone:tel:", "") || ""),
    website: websiteNode?.href || "",
    mapsUrl,
    rating: parseNumber(ratingLabel || ""),
    reviewCount: parseNumber(reviewLabel || ""),
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

    const placeUrls = await collectPlaceUrls(page, maxResults);
    if (!placeUrls.length) {
      throw new Error("没有找到商家。请检查关键词、地区或浏览器中的提示。");
    }
    emit("progress", {
      discovered: placeUrls.length,
      message: `发现 ${placeUrls.length} 家商户，开始读取详情`,
    });

    let completed = 0;
    for (const mapsUrl of placeUrls) {
      await page.goto(mapsUrl, { waitUntil: "domcontentloaded", timeout: 35_000 });
      await waitForManualVerification(page);
      await page.waitForTimeout(650);
      try {
        const data = await extractBusiness(page, mapsUrl);
        if (data.name) emit("business", { data });
        else emit("error", { message: "跳过一个无法识别名称的商家" });
      } catch (error) {
        emit("error", { message: `读取商家详情失败：${error.message}` });
      }
      completed += 1;
      emit("progress", {
        discovered: placeUrls.length,
        message: `已读取 ${completed}/${placeUrls.length} 家商户详情`,
      });
    }
    emit("done", { discovered: placeUrls.length });
  } finally {
    await close();
  }
}

main().catch((error) => {
  emit("error", { message: error?.message || String(error) });
  process.exitCode = 1;
});
