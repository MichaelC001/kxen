import { Route, Router } from "@solidjs/router";
import Sidebar from "./components/Sidebar";
import Session from "./pages/Session";
import Goals from "./pages/Goals";
import Tasks from "./pages/Tasks";
import Doctor from "./pages/Doctor";

function Layout(props: { children?: unknown }) {
  return (
    <div class="h-screen flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 min-w-0">{props.children}</main>
    </div>
  );
}

export default function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Session} />
      <Route path="/goals" component={Goals} />
      <Route path="/tasks" component={Tasks} />
      <Route path="/doctor" component={Doctor} />
    </Router>
  );
}
