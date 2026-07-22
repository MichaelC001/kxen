// ThinkingOrb：点阵思考球（canvas 2D，rAF 共享时钟，离屏/隐藏自停，reduced-motion 静态帧）。
import { onCleanup, onMount } from "solid-js";
import { drawOrbFrame, ORB_ARIA, type OrbSize, type OrbState } from "../lib/orb";
import { theme } from "../lib/theme";

export default function ThinkingOrb(props: {
  state: () => OrbState;
  size?: OrbSize;
  speed?: number;
  paused?: boolean;
}) {
  let canvas: HTMLCanvasElement | undefined;
  let raf = 0;
  let visible = true;
  let io: IntersectionObserver | undefined;
  const size = () => props.size ?? 64;
  const reduced = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const paintOnce = () => {
    const ctx = canvas?.getContext("2d");
    if (!ctx || !canvas) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const s = size();
    if (canvas.width !== s * dpr) {
      canvas.width = s * dpr;
      canvas.height = s * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    }
    drawOrbFrame(ctx, props.state(), s, 0, theme() === "dark");
  };

  onMount(() => {
    const ctx = canvas?.getContext("2d");
    if (!ctx || !canvas) return;
    if (reduced()) {
      paintOnce();
      return;
    }
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const s = size();
    canvas.width = s * dpr;
    canvas.height = s * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const t0 = performance.now();
    const loop = (now: number) => {
      raf = requestAnimationFrame(loop);
      if (!visible || props.paused || document.hidden) return;
      const t = ((now - t0) / 1000) * (props.speed ?? 1);
      drawOrbFrame(ctx, props.state(), s, t, theme() === "dark");
    };
    raf = requestAnimationFrame(loop);
    io = new IntersectionObserver((entries) => {
      visible = entries[0]?.isIntersecting ?? true;
    });
    if (canvas) io.observe(canvas);
  });

  onCleanup(() => {
    cancelAnimationFrame(raf);
    io?.disconnect();
  });

  return (
    <canvas
      ref={(el) => (canvas = el)}
      width={size()}
      height={size()}
      style={`width:${size()}px;height:${size()}px`}
      role="img"
      aria-label={ORB_ARIA[props.state()]}
    />
  );
}
