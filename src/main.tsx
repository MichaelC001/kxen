import { render } from "solid-js/web";
import App from "./App";
import WebTokenGuide from "./components/WebTokenGuide";
import { resolveWebToken } from "./lib/client-endpoint";
import { isWeb } from "./lib/runtime";
import { initTheme } from "./lib/theme";
import "./styles.css";

initTheme();

// 浏览器模式缺 token：连接必然被拒，停在引导页而不是让各面板各自加载失败
const guide = isWeb() && !resolveWebToken() ? WebTokenGuide : App;
render(() => guide(), document.getElementById("root")!);
