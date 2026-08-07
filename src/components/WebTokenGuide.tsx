// 浏览器模式缺 token 引导页：后端无 token 拒绝一切连接，停在明确状态而不是全线「加载失败」。
// token 来源只有两个对外通道：kxen-web 启动 stdout 打印的完整链接、桌面端托盘「复制访问链接」。
export default function WebTokenGuide() {
  return (
    <div class="h-screen flex items-center justify-center bg-[var(--bg)]">
      <div class="max-w-md rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-6 space-y-3">
        <div class="text-lg font-semibold tracking-tight text-[var(--accent-hover)]">kxen</div>
        <div class="text-sm text-[var(--text)]">需要带访问令牌的链接</div>
        <p class="text-xs leading-relaxed text-[var(--text-dim)]">
          浏览器访问必须使用带 token 的完整链接（形如 http://主机：端口/?token=...）， 链接中的
          token 读取后会自动从地址栏抹除并记住在本浏览器中。
        </p>
        <p class="text-xs leading-relaxed text-[var(--text-dim)]">
          获取方式：服务端启动日志会打印完整链接（kxen-web 启动时输出）；
          桌面端可从系统托盘菜单「复制访问链接」获取。
        </p>
      </div>
    </div>
  );
}
