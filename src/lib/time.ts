export function relTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  const days = Math.floor(diff / 86_400_000);
  if (days === 1) return "昨天";
  if (days < 30) return `${days} 天前`;
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()}`;
}
