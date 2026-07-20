import { createWriteStream } from 'node:fs';
import * as pureimage from 'pureimage';

export interface SnapFrame {
	path: string;
	chars: number;
}

export interface SnapRenderOptions {
	frameWidth?: number;
	fontSize?: number;
	// 估算：一帧能容纳的字符数上限
	maxCharsPerFrame?: number;
}

const CHARS_PER_LINE_FALLBACK = 80;

function wrap(text: string, width: number): string[] {
	const lines: string[] = [];
	for (const raw of text.split('\n')) {
		if (raw.length <= width) {
			lines.push(raw);
		} else {
			for (let i = 0; i < raw.length; i += width) {
				lines.push(raw.slice(i, i + width));
			}
		}
	}
	return lines;
}

// snapcompact 纯 TS 渲染（OMP 思路，pureimage 实现）：
// 把被丢弃的历史渲染成像素字 PNG 帧，vision 模型直接读回，零模型调用、零网络
export async function renderTextToFrames(
	text: string,
	outPath: (index: number) => string,
	opts: SnapRenderOptions = {},
): Promise<SnapFrame[]> {
	const frameWidth = opts.frameWidth ?? 1024;
	const fontSize = opts.fontSize ?? 14;
	const charWidth = Math.max(1, Math.floor(fontSize * 0.6));
	const lineHeight = Math.floor(fontSize * 1.2);
	const charsPerLine =
		Math.floor(frameWidth / charWidth) || CHARS_PER_LINE_FALLBACK;
	const linesPerFrame = Math.floor(frameWidth / lineHeight);
	const lines = wrap(text, charsPerLine);

	const frames: SnapFrame[] = [];
	for (let start = 0; start < lines.length; start += linesPerFrame) {
		const chunk = lines.slice(start, start + linesPerFrame);
		const img = pureimage.make(frameWidth, lineHeight * chunk.length);
		const ctx = img.getContext('2d');
		ctx.fillStyle = '#ffffff';
		ctx.fillRect(0, 0, img.width, img.height);
		ctx.fillStyle = '#000000';
		ctx.font = `${fontSize}px monospace`;
		chunk.forEach((line, i) => {
			ctx.fillText(line, 0, lineHeight * (i + 1));
		});
		const path = outPath(frames.length);
		await pureimage.encodePNGToStream(img, createWriteStream(path));
		frames.push({ path, chars: chunk.join('\n').length });
	}
	return frames;
}
