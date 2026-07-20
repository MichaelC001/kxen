import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const KXEN_AGENT_DIR = join(homedir(), '.kxen', 'agent');
export const KXEN_AUTH_PATH = join(KXEN_AGENT_DIR, 'auth.json');

export function ensureAgentDir(): string {
	mkdirSync(KXEN_AGENT_DIR, { recursive: true });
	ensurePromptTemplates();
	return KXEN_AGENT_DIR;
}

// slash commands 走 pi 的 prompt templates 机制（<agentDir>/prompts/*.md），不自建命令分发
const PROMPT_TEMPLATES: Record<string, string> = {
	'goal.md': `---
description: 创建并推进一个 goal（状态机 + 验证循环）
---
用 GoalEngine 为以下目标创建 goal contract（objective + completionCriteria + constraints），激活后用 runGoal 推进直到验证通过或阻塞。目标内容：

$ARGUMENTS
`,
	'workflow.md': `---
description: 为任务生成并运行一个 workflow 编排脚本
---
用 WorkflowRuntime 为以下任务写 workflow 脚本（agent()/pipeline()/constraints()），fan-out 合理规模，执行并汇总结果。任务内容：

$ARGUMENTS
`,
};

function ensurePromptTemplates(): void {
	const dir = join(KXEN_AGENT_DIR, 'prompts');
	mkdirSync(dir, { recursive: true });
	for (const [name, content] of Object.entries(PROMPT_TEMPLATES)) {
		const path = join(dir, name);
		if (!existsSync(path)) writeFileSync(path, content);
	}
}
