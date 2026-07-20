# 灾难操作防护规则集

- 日期: 2026-07-20
- 依据: prd 3.7 安全模型；三类（毁系统 / 毁用户目录 / 删 git 仓库）为代表例，本文档给出完整清单
- 原则: 防护在代码层（规则引擎 + 路径守卫 + 命令解析），不在提示词；`forbidden` 决策不可被 prompt、AGENTS.md、项目规则覆盖

## 1. 决策分层

| 档位 | 语义 | 处置 |
| --- | --- | --- |
| forbidden | 不可逆灾难，编码场景无正当用途 | 机器直接拒绝，给出原因；不支持批准后固化；用户只能在配置里显式删规则 |
| approval | 高风险但有正当用途 | 交互中问用户，一次性批准；可按 T11 固化为 allow |
| allow | 常规操作 | 直接执行 |

forbidden 与 approval 的关键区别不是「危险程度」，而是「在编码会话中是否存在正当用途」：删 `.git` 没有正当用途，`git push --force` 有（自己的 WIP 分支）。

## 2. forbidden 规则族（机器拒绝）

### F1 系统毁灭

- 删除或改写系统路径: `/`、`/System`、`/usr`、`/bin`、`/sbin`、`/etc`、`/var`、`/Library`、`/private`、`/boot`、`/proc`、`/sys`、`/dev`（含 `rm` / `mv` / `rsync --delete` / `find -delete` / 重定向写入）
- 磁盘级操作: `dd of=/dev/*`、`mkfs.*`、`diskutil erase*`、`hdiutil erase*`、`fdisk`、`parted`
- 系统属性修改: `chmod -R` / `chown -R` 作用于系统路径、`nvram`、`csrutil`
- 系统级进程: 杀 init / launchd / systemd / WindowServer、`shutdown`、`reboot`、fork bomb 模式

### F2 用户目录毁灭

- 删除 `$HOME` 本身或其一级目录: `~/Documents`、`~/Desktop`、`~/Downloads`、`~/Library`、`~/Pictures`、`~/Movies`、`~/.ssh`、`~/.gnupg`、`~/.aws`、`~/.config`、`~/.kube`、`~/.docker`、shell rc 文件
- 凭证存储销毁: macOS Keychain 删除项（`security delete-*`）、`gpg --delete-secret-key`、删除密码管理器数据库
- 浏览器 profile 目录整体删除

注意: 工作区（用户当前项目目录）内的删除不属于此类；`$HOME` 下普通文件的正常编辑不受限。

### F3 git 仓库毁灭

- 删除 `.git` 目录或仓库根目录（含 `rm` / `mv` 到废纸篓的等价操作 / `find . -name .git -delete`）
- 批量删除 refs: `git update-ref -d` 作用于全部或大量分支 / tag、`git branch -D` 全部分支
- 删除 remote 的裸仓库内容（`git push --delete` 全部 refs）

正常 git 操作（reset / clean / checkout / 删单个分支）走 approval 档，不在此列。

### F4 数据与基础设施毁灭

- 数据库: `DROP DATABASE` / `DROP SCHEMA`、删除整实例（`dropdb`、`mongod --eval dropDatabase` 作用于非临时库）
- IaC 与云: `terraform destroy`、`aws/gcloud/az` 的删除类命令（`aws s3 rb --force`、`gcloud projects delete`）、`kubectl delete ns` / `delete --all`
- 容器与卷: `docker system prune --volumes`、删除 named volume
- 挂载卷 / 外接存储上的递归删除

### F5 批量失控类

- 无路径限定的递归删除: `rm -rf` 目标是 `/`、`~`、变量未定义展开为空导致的根级删除（如 `rm -rf $DIR/` 且 `DIR` 为空）
- 面向全部子代理 / 全部会话的批量不可逆指令在 workflow 层同样过此规则集（编排层无豁免）

## 3. approval 规则族（问用户）

- git: `push --force` / `--force-with-lease`、`reset --hard`、`clean -fdx`、`rebase`、删除分支、改 CI 配置
- 文件: 删除未跟踪 / 未提交的修改、覆盖写入非本任务创建的文件
- 进程: kill 非本 harness 启动的进程
- 包管理: 全局安装 / 卸载、lockfile 大幅变更
- 网络外发: 向第三方服务 POST / PUT 数据（pastebin、webhook、上传制品）
- 提权: 任何 `sudo` / `doas` 命令（默认 approval，可显式 forbidden）

## 4. 防绕过设计（关键）

规则匹配必须在三个位置同时成立，单层必被绕过：

1. 命令解析层: 静态拆解管道 / `&&` / `;` 分段，识别 `sudo` / `xargs` / `find -exec` / `bash -c` 嵌套、重定向目标文件；变量无法静态求值时降级为 approval 而不是 allow
2. 路径守卫层（最终防线）: write / edit / delete / bash 的实际路径在运行时 resolve（绝对化 + 解 symlink）后与保护清单比对；命中 forbidden 清单的路径操作无论经由哪条命令一律拒绝
3. 审计层: 所有 forbidden 命中与 approval 决策进事件流（谁发起、命令、路径、决策、结果），可回放

补充约束:

- 路径保护清单支持 glob（如 `~/Library/**`、`**/.git`），项目级可追加但不可移除内置清单
- forbidden 命中给模型的返回是结构化错误（规则 id + 原因 + 可替代动作建议），让模型能换路而不是重试

## 5. 与既有文档的关系

- prd 3.7: 安全模型总述，本文档是其规则集落地
- analysis/02 T10 / T11: execpolicy 引擎与批准后固化机制，本清单作为内置规则包注入
- analysis/07 P11: prompt 只陈述机制边界，本清单不出现在系统提示词里（只在被拒错误的返回中对模型可见）
