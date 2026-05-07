import { chromium } from "playwright";
const expr = process.argv[2] ?? "1+1";
const browser = await chromium.connectOverCDP("http://localhost:9222");
const ctx = browser.contexts()[0];
const page = ctx.pages()[0] ?? (await ctx.newPage());
const result = await page.evaluate(expr);
console.log(JSON.stringify(result, null, 2));
await browser.close();
