import { Route, Router } from "@solidjs/router";
import Sidebar from "./components/Sidebar";
import Dock from "./components/Dock";
import Session from "./pages/Session";
import Doctor from "./pages/Doctor";

function Home() {
  return (
    <>
      <Session />
      <Dock />
    </>
  );
}

function Layout(props: { children?: unknown }) {
  return (
    <div class="h-screen flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 min-w-0 flex">{props.children}</main>
    </div>
  );
}

export default function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/doctor" component={Doctor} />
    </Router>
  );
}
