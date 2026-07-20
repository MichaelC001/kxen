import path from "path"
import { Global } from "@kxen/core/global"
import { ModelResourceManager, loadConfig } from "@kxen/mrm"

// 全局单例：daemon 生命周期内共享资源视图（并发/排队/降级状态）。
// 用户级配置 ~/.config/kxen/config.toml；项目级 .kxen/config.toml 后续并入。
let instance: Promise<ModelResourceManager> | undefined

export function mrmInstance(): Promise<ModelResourceManager> {
  instance ??= loadConfig({ user: path.join(Global.Path.config, "config.toml") }).then(
    (c) => new ModelResourceManager(c),
  )
  return instance
}
