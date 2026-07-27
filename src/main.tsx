import { render } from "solid-js/web";
import App from "./App";
import { initTheme } from "./lib/theme";
import "./styles.css";

initTheme();

render(() => <App />, document.getElementById("root")!);
