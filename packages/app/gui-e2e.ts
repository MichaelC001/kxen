// proof 第 2 条：浏览器里创建会话并完成一次真实模型调用（URL 直达会话页）。
import { chromium } from "@playwright/test"

const DIR = "L1VzZXJzL3hpYW9iYWkvQ29kZS9TZWxmQ29kZS9reGVu" // base64(/Users/xiaobai/Code/SelfCode/kxen)
const browser = await chromium.launch({ channel: "chrome" })
const page = await browser.newPage()

try {
  await page.goto(`http://localhost:3000/${DIR}/session`, { waitUntil: "domcontentloaded", timeout: 30000 })
  await page.waitForLoadState("networkidle", { timeout: 20000 }).catch(() => {})

  const promptBox = page.locator("[contenteditable='true'], [role='textbox']").first()
  await promptBox.waitFor({ state: "visible", timeout: 20000 })
  await promptBox.click()
  await page.keyboard.type("Reply with exactly: kxen-gui-ok")
  await page.keyboard.press("Enter")

  const reply = page.locator("text=kxen-gui-ok").first()
  const found = await reply.waitFor({ state: "visible", timeout: 120000 }).then(() => true).catch(() => false)

  await page.screenshot({ path: "gui-e2e.png" })
  if (!found) {
    console.error("FAIL: 回复中未出现 kxen-gui-ok")
    process.exit(1)
  }
  console.log("PASS: 浏览器创建会话并完成一次真实模型调用")
} finally {
  await page.screenshot({ path: "gui-e2e-final.png" }).catch(() => {})
  await browser.close()
}
