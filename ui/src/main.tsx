import { render } from "solid-js/web";
import App from "./App";
import { initMarkdown } from "./lib/markdown";
import { initTheme } from "./lib/theme";
import "./styles.css";

initTheme();
// shiki/mermaid 初始化与首帧并行；未就绪前代码块退化为纯文本
void initMarkdown();

render(() => <App />, document.getElementById("root")!);
