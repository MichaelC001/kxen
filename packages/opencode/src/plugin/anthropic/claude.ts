import type { Hooks, PluginInput } from "@kxen/plugin"
import { OAUTH_DUMMY_KEY } from "../../auth"

// Claude Pro/Max 订阅（Claude Code OAuth）接入。
// 凭证来源优先级：官方 Claude Code 存储（Keychain / ~/.claude/.credentials.json）新鲜副本
// 由 src/auth/import.ts 导入到 auth.json；本插件负责调用期注入与过期刷新。
const CLIENT_ID = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
const TOKEN_URL = "https://console.anthropic.com/v1/oauth/token"
const OAUTH_BETA = "oauth-2025-04-20"

interface TokenResponse {
  access_token: string
  refresh_token: string
  expires_in?: number
}

async function refreshAccessToken(refreshToken: string): Promise<TokenResponse> {
  const response = await fetch(TOKEN_URL, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      grant_type: "refresh_token",
      refresh_token: refreshToken,
      client_id: CLIENT_ID,
    }),
  })
  if (!response.ok) {
    throw new Error(`Token refresh failed: ${response.status}`)
  }
  return response.json()
}

export const ClaudeAuthPlugin = async (input: PluginInput): Promise<Hooks> => {
  return {
    auth: {
      provider: "anthropic",
      methods: [],
      async loader(getAuth) {
        const auth = await getAuth()
        if (auth.type !== "oauth") return {}

        let refreshPromise: Promise<{ access: string }> | undefined

        return {
          apiKey: OAUTH_DUMMY_KEY,
          async fetch(requestInput: RequestInfo | URL, init?: RequestInit) {
            const currentAuth = await getAuth()
            if (currentAuth.type !== "oauth") return fetch(requestInput, init)

            if (!currentAuth.access || currentAuth.expires < Date.now()) {
              if (!refreshPromise) {
                refreshPromise = refreshAccessToken(currentAuth.refresh)
                  .then(async (tokens) => {
                    const expires = Date.now() + (tokens.expires_in ?? 3600) * 1000
                    await input.client.auth.set({
                      path: { id: "anthropic" },
                      body: {
                        type: "oauth",
                        refresh: tokens.refresh_token,
                        access: tokens.access_token,
                        expires,
                      },
                    })
                    return { access: tokens.access_token }
                  })
                  .finally(() => {
                    refreshPromise = undefined
                  })
              }
              const refreshed = await refreshPromise
              currentAuth.access = refreshed.access
            }

            const headers = new Headers(init?.headers as HeadersInit | undefined)
            headers.set("authorization", `Bearer ${currentAuth.access}`)
            headers.delete("x-api-key")
            const beta = headers.get("anthropic-beta")
            headers.set("anthropic-beta", beta ? `${OAUTH_BETA},${beta}` : OAUTH_BETA)

            return fetch(requestInput, { ...init, headers })
          },
        }
      },
    },
  }
}
