// TrajectoryList 虚拟化实测：定高条目 + spacer，只挂可视窗；跟随尾部、触顶自动加载更早、锚点恢复。
// 浏览器模式真实布局：条目高 24px（row）/ 9px（sep）由 style 保证。测试环境不加载 Tailwind，
// 应用里由 flex-1/overflow-auto 建立的滚动容器契约（定高 + 可滚动）在此用 inline style 显式重建。
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import TrajectoryList from "./TrajectoryList";
import { groupTrajectoryTurns, type TrajectoryRecord } from "../lib/trajectory";

function rec(i: number): TrajectoryRecord {
  return {
    index: i,
    kind: "user",
    messageId: `m${i}`,
    partIndex: 0,
    time: i,
    role: "user",
    summary: `消息第${i}条`,
  };
}

const ALL = Array.from({ length: 150 }, (_, i) => rec(i));

function mount(initial: TrajectoryRecord[], all: TrajectoryRecord[]) {
  const [records, setRecords] = createSignal(initial);
  const onLoadEarlier = vi.fn(() => setRecords(all));
  const turns = () => groupTrajectoryTurns(records());
  const dispose = render(
    () => (
      <TrajectoryList
        turns={turns}
        collapseTurns={() => false}
        collapseCalls={() => false}
        showDuration={() => false}
        expandedTurns={() => new Set<number>()}
        onToggleTurn={() => {}}
        selectedIndex={() => undefined}
        onSelect={() => {}}
        hasEarlier={() => records().length < all.length}
        earlierLabel={() => "加载更早记录"}
        onLoadEarlier={onLoadEarlier}
        focusIndex={() => undefined}
      />
    ),
    document.body,
  );
  const el = document.body.querySelector<HTMLDivElement>("[data-testid='trajectory-list']")!;
  el.style.height = "200px";
  el.style.overflow = "auto";
  const list = () =>
    document.body.querySelector<HTMLDivElement>("[data-testid='trajectory-list']")!;
  return { dispose, list, onLoadEarlier, setRecords };
}

const rows = () => [...document.body.querySelectorAll("[data-testid='trajectory-row']")];
const scrollTo = (el: HTMLElement, top: number) => {
  el.scrollTop = top;
  el.dispatchEvent(new Event("scroll"));
};

afterEach(() => {
  document.body.innerHTML = "";
});

describe("TrajectoryList 虚拟化", () => {
  it("150 条记录只挂载可视窗行，初载定位最新尾部", async () => {
    const { dispose, list } = mount(ALL, ALL);
    // ResizeObserver 测得 viewport=200 后窗口化生效
    await vi.waitFor(() => expect(rows().length).toBeLessThan(50));
    expect(rows().length).toBeGreaterThan(0);
    // 跟随尾部：初载定位最新
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第149条"));
    expect(list().scrollTop).toBeGreaterThan(0);
    expect(document.body.textContent).not.toContain("消息第0条");
    dispose();
  });

  it("上滚暂停跟随：新记录只涨 spacer 不打断检视；回底恢复跟随", async () => {
    const { dispose, list, setRecords } = mount(ALL, ALL);
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第149条"));
    // 上滚到中部：脱离跟随
    scrollTo(list(), 500);
    expect(list().scrollTop).toBe(500);
    // 新记录到达：视口不动，新行不进入视口
    setRecords([...ALL, rec(150)]);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(list().scrollTop).toBe(500);
    expect(document.body.textContent).not.toContain("消息第150条");
    // 回底恢复跟随：再追加一条，自动定位尾部
    scrollTo(list(), list().scrollHeight);
    setRecords([...ALL, rec(150), rec(151)]);
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第151条"));
    dispose();
  });

  it("触顶自动加载更早一页，锚点恢复后视口内容原位不动", async () => {
    // 初始只挂尾部 100 条（index 50..149），hasEarlier = true
    const { dispose, list, onLoadEarlier } = mount(ALL.slice(50), ALL);
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第149条"));
    // 滚到顶部触发自动加载
    scrollTo(list(), 0);
    await vi.waitFor(() => expect(onLoadEarlier).toHaveBeenCalledTimes(1));
    // prepend 后锚点恢复：原视口首条（消息第50条）仍在视口内
    // 50 row + 50 sep 在新序列里的顶部偏移 = 50*24 + 50*9 = 1650
    await vi.waitFor(() => expect(list().scrollTop).toBe(1650));
    expect(document.body.textContent).toContain("消息第50条");
    expect(document.body.textContent).not.toContain("消息第0条");
    dispose();
  });

  it("加载更早完成后 hasEarlier 变 false，顶部按钮消失", async () => {
    const { dispose, list } = mount(ALL.slice(50), ALL);
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第149条"));
    const btn = () =>
      document.body.querySelector<HTMLButtonElement>("[data-testid='trajectory-load-earlier']")!;
    expect(btn().textContent).toBe("加载更早记录");
    scrollTo(list(), 0);
    await vi.waitFor(() =>
      expect(document.body.querySelector("[data-testid='trajectory-load-earlier']")).toBeNull(),
    );
    dispose();
  });
});
