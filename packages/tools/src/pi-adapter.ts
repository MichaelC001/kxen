import {
	defineTool,
	type ToolDefinition,
} from '@earendil-works/pi-coding-agent';
import { isPlanExecAllowed } from '@kxen/core';
import { Type } from 'typebox';
import { DIALECT_CARDS, exec } from './exec';
import { editTextFile, readTextFile, writeTextFile } from './fs';

const ok = (text: string) => ({
	content: [{ type: 'text' as const, text }],
	details: undefined,
});

export const readTool: ToolDefinition = defineTool({
	name: 'read',
	label: 'Read',
	description: '读取文件内容，带行号。大文件用 offset / limit 分页。',
	parameters: Type.Object({
		path: Type.String({ description: '文件路径（相对或绝对）' }),
		offset: Type.Optional(Type.Number({ description: '起始行（1 起始）' })),
		limit: Type.Optional(Type.Number({ description: '行数上限' })),
	}),
	async execute(_id, params) {
		const r = readTextFile(params.path, {
			offset: params.offset,
			limit: params.limit,
		});
		return ok(r.content);
	},
});

export const writeTool: ToolDefinition = defineTool({
	name: 'write',
	label: 'Write',
	description: '写入文件（不存在则创建，存在则覆盖），自动创建父目录。',
	parameters: Type.Object({
		path: Type.String(),
		content: Type.String(),
	}),
	async execute(_id, params) {
		writeTextFile(params.path, params.content);
		return ok(`已写入 ${params.path}`);
	},
});

export const editTool: ToolDefinition = defineTool({
	name: 'edit',
	label: 'Edit',
	description:
		'精确替换文件中的文本。oldText 必须唯一匹配（含空白）；文件须先 read。',
	parameters: Type.Object({
		path: Type.String(),
		oldText: Type.String(),
		newText: Type.String(),
	}),
	async execute(_id, params) {
		editTextFile(params.path, params.oldText, params.newText);
		return ok(`已编辑 ${params.path}`);
	},
});

const shellTypeSchema = Type.Union([
	Type.Literal('zsh'),
	Type.Literal('bash'),
	Type.Literal('fish'),
	Type.Literal('cmd'),
	Type.Literal('powershell'),
]);

export function createExecTool(readOnly: boolean): ToolDefinition {
	return defineTool({
		name: 'exec',
		label: 'Exec',
		description: `在指定 shell 中执行命令。type 必填，按 type 自查方言：${Object.values(DIALECT_CARDS).join(' | ')}。输出超长时截断首尾并落盘。${readOnly ? '当前为只读模式，仅允许白名单命令。' : ''}`,
		parameters: Type.Object({
			type: shellTypeSchema,
			path: Type.String({ description: '工作目录' }),
			command: Type.String(),
			timeout: Type.Optional(Type.Number({ description: '毫秒' })),
			background: Type.Optional(Type.Boolean()),
		}),
		async execute(_id, params) {
			if (readOnly && !isPlanExecAllowed(params.command)) {
				return ok(
					`命令被拒：只读模式仅允许白名单命令（ls / git status / rg 等）`,
				);
			}
			const r = await exec(params);
			const note = r.truncated ? `\n[输出已截断，全文: ${r.outputFile}]` : '';
			return ok(`${r.stdout}${note}\nexit: ${r.exitCode}`);
		},
	});
}

export function createKxenTools(
	opts: { execReadOnly?: boolean } = {},
): ToolDefinition[] {
	return [
		readTool,
		writeTool,
		editTool,
		createExecTool(opts.execReadOnly ?? false),
	];
}
