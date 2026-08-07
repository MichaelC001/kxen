# 安全策略

## 支持范围

| 版本     | 状态                   |
| -------- | ---------------------- |
| 0.0.x    | 开发预览，接受安全修复 |
| 其他版本 | 不支持                 |

## 报告漏洞

请通过 GitHub Private Vulnerability Reporting 提交漏洞:

https://github.com/StringKe/kxen/security/advisories/new

报告至少包含受影响版本、复现步骤、影响范围和已知缓解方式。不要在公开 Issue 中披露未修复漏洞或凭证。

`kxen` 无头 server 可以绑定到局域网或公网地址，带 token 的访问 URL 是其唯一认证机制，这属于设计行为；`kxen` 自身不实现 TLS，远程部署应经 tailscale 等终结 TLS。绕过 token 校验、Host 白名单或 Origin 检查属于有效的安全报告范围，token 泄露后的访问不属于漏洞。

维护者确认报告后会在 3 个工作日内给出初步响应，并在修复可用后协调披露。
