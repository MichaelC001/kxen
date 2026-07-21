import { createSignal, Show } from "solid-js";
import Doctor from "./pages/Doctor";
import Session from "./pages/Session";

const PAGES = [
  { hash: "#/", label: "会话" },
  { hash: "#/doctor", label: "Doctor" },
] as const;

export default function App() {
  const [route, setRoute] = createSignal(window.location.hash || "#/");
  window.addEventListener("hashchange", () => setRoute(window.location.hash || "#/"));

  return (
    <div class="h-screen flex">
      <nav class="w-44 border-r border-gray-800 p-3 space-y-1 shrink-0">
        <div class="px-2 py-1 text-lg font-bold text-indigo-400">kxen</div>
        {PAGES.map((p) => (
          <a
            href={p.hash}
            class={`block px-2 py-1.5 rounded text-sm ${route() === p.hash ? "bg-gray-800 text-white" : "text-gray-400 hover:bg-gray-800/50"}`}
          >
            {p.label}
          </a>
        ))}
      </nav>
      <main class="flex-1 overflow-auto">
        <Show when={route() === "#/doctor"} fallback={<Session />}>
          <Doctor />
        </Show>
      </main>
    </div>
  );
}
