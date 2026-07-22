import { Route, Router } from "@solidjs/router";
import Sidebar from "./components/Sidebar";
import RightColumn from "./components/RightColumn";
import StatusBar from "./components/StatusBar";
import CommandPalette from "./components/CommandPalette";
import Session from "./pages/Session";
import Settings from "./pages/Settings";
import { hasConversation, setNavigator } from "./lib/state";
import { mountShortcuts } from "./lib/shortcuts";
import { useNavigate } from "@solidjs/router";
import { onCleanup, onMount } from "solid-js";

function Home() {
  return (
    <>
      <div class="flex-1 min-w-0 flex flex-col">
        <div class="flex-1 min-h-0 flex">
          <Session />
          <div class="dock-wrap" classList={{ "dock-hidden": !hasConversation() }}>
            <RightColumn />
          </div>
        </div>
        <StatusBar />
      </div>
    </>
  );
}

function Layout(props: { children?: import("solid-js").JSX.Element }) {
  const navigate = useNavigate();
  setNavigator(navigate);
  let unmount: (() => void) | undefined;
  onMount(() => {
    unmount = mountShortcuts();
  });
  onCleanup(() => unmount?.());
  return (
    <div class="h-screen flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 min-w-0 flex">{props.children}</main>
      <CommandPalette />
    </div>
  );
}

export default function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/settings" component={Settings} />
    </Router>
  );
}
