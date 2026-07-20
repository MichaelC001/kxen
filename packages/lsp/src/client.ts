import { type ChildProcess, spawn } from 'node:child_process';
import {
	createMessageConnection,
	type MessageConnection,
	StreamMessageReader,
	StreamMessageWriter,
} from 'vscode-jsonrpc/node';
import type { ResolvedServer } from './detect';

export interface LspPosition {
	line: number;
	character: number;
}

export interface LspLocation {
	uri: string;
	range: { start: LspPosition; end: LspPosition };
}

export interface LspDiagnostic {
	severity?: number;
	message: string;
	range: { start: LspPosition; end: LspPosition };
	source?: string;
}

// 原生 LSP 客户端（analysis/09 L1）：vscode-jsonrpc 承载协议，不自造
export class LspClient {
	private proc?: ChildProcess;
	private conn?: MessageConnection;
	private diagnosticsHandler?: (
		uri: string,
		diagnostics: LspDiagnostic[],
	) => void;

	constructor(private server: ResolvedServer) {}

	async start(rootUri: string): Promise<void> {
		this.proc = spawn(this.server.binaryPath, this.server.args, {
			stdio: ['pipe', 'pipe', 'pipe'],
		});
		this.conn = createMessageConnection(
			new StreamMessageReader(this.proc.stdout!),
			new StreamMessageWriter(this.proc.stdin!),
		);
		this.conn.onNotification(
			'textDocument/publishDiagnostics',
			(params: { uri: string; diagnostics: LspDiagnostic[] }) => {
				this.diagnosticsHandler?.(params.uri, params.diagnostics);
			},
		);
		this.conn.listen();
		await this.conn.sendRequest('initialize', {
			processId: process.pid,
			rootUri,
			capabilities: {
				textDocument: {
					definition: { linkSupport: false },
					references: {},
					hover: {},
					documentSymbol: { hierarchicalDocumentSymbolSupport: true },
					rename: {},
					publishDiagnostics: {},
				},
			},
			clientInfo: { name: 'kxen', version: '0.0.0' },
		});
		await this.conn.sendNotification('initialized', {});
	}

	onDiagnostics(
		handler: (uri: string, diagnostics: LspDiagnostic[]) => void,
	): void {
		this.diagnosticsHandler = handler;
	}

	async definition(uri: string, position: LspPosition): Promise<LspLocation[]> {
		return this.request('textDocument/definition', {
			textDocument: { uri },
			position,
		});
	}

	async references(uri: string, position: LspPosition): Promise<LspLocation[]> {
		return this.request('textDocument/references', {
			textDocument: { uri },
			position,
			context: { includeDeclaration: true },
		});
	}

	async hover(uri: string, position: LspPosition): Promise<unknown> {
		return this.request('textDocument/hover', {
			textDocument: { uri },
			position,
		});
	}

	async documentSymbols(uri: string): Promise<unknown> {
		return this.request('textDocument/documentSymbol', {
			textDocument: { uri },
		});
	}

	async rename(
		uri: string,
		position: LspPosition,
		newName: string,
	): Promise<unknown> {
		return this.request('textDocument/rename', {
			textDocument: { uri },
			position,
			newName,
		});
	}

	async didOpen(uri: string, text: string): Promise<void> {
		await this.notify('textDocument/didOpen', {
			textDocument: {
				uri,
				languageId: this.server.languageId,
				version: 1,
				text,
			},
		});
	}

	async didChange(uri: string, text: string, version: number): Promise<void> {
		await this.notify('textDocument/didChange', {
			textDocument: { uri, version },
			contentChanges: [{ text }],
		});
	}

	private requireConn(): MessageConnection {
		if (!this.conn) throw new Error('LSP 未启动');
		return this.conn;
	}

	private async request<T>(method: string, params: unknown): Promise<T> {
		return (await this.requireConn().sendRequest(method, params)) as T;
	}

	private async notify(method: string, params: unknown): Promise<void> {
		await this.requireConn().sendNotification(method, params);
	}

	async shutdown(): Promise<void> {
		try {
			if (this.conn) {
				await this.conn.sendRequest('shutdown');
				await this.conn.sendNotification('exit');
				this.conn.dispose();
			}
		} finally {
			this.proc?.kill();
			this.conn = undefined;
			this.proc = undefined;
		}
	}
}
