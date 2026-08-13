export function decodeArtifactPreview(contentBase64: string, mediaType: string): string {
  const bytes = Uint8Array.from(atob(contentBase64), (character) => character.charCodeAt(0));
  return mediaType.startsWith("text/") || mediaType.includes("json")
    ? new TextDecoder().decode(bytes)
    : `已验证 ${bytes.byteLength} bytes 的 ${mediaType} 内容。二进制内容不在预览区渲染。`;
}
