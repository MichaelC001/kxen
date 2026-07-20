// M2 GUI 冒烟：验证 vite 页面可加载、前端能连上 daemon、无致命 console 错误。
// 前置：daemon 在 :4096、vite dev 在 :3000。
import { chromium } from "@playwright/test"

const errors: string[] = []
const browser = await chromium.launch({ channel: "chrome" })
const page = await browser.newPage()
page.on("console", (msg) => {
  if (msg.type() === "error") errors.push(msg.text().slice(0, 200))
})
page.on("pageerror", (err) => errors.push(String(err).slice(0, 200)))

await page.goto("http://localhost:3000/", { waitUntil: "domcontentloaded", timeout: 30000 })
// 前端启动后会向 daemon 拉数据，等网络空闲
await page.waitForLoadState("networkidle", { timeout: 30000 }).catch(() => {})

const title = await page.title()
const rootChildren = await page.locator("#root").evaluate((el) => el.childElementCount)
await page.screenshot({ path: "gui-smoke.png", fullPage: false })
const bodyText = await page.locator("body").innerText().catch(() => "")
console.log("BODY:", bodyText.slice(0, 600))
await browser.close()

const fatal = errors.filter(
  (e) => !e.includes("favicon") && !e.includes("DevTools") && !e.includes("fonts.g"),
)
console.log(JSON.stringify({ title, rootChildren, consoleErrors: fatal.slice(0, 5) }, null, 2))
if (rootChildren === 0) {
  console.error("FAIL: #root 为空，SPA 未挂载")
  process.exit(1)
}
if (fatal.length > 0) {
  console.error(`FAIL: ${fatal.length} 条 console 错误`)
  process.exit(1)
}
console.log("PASS: GUI 冒烟通过")
