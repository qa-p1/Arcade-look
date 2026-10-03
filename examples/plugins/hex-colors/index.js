export async function render(host, ctx) {
  const text = await ctx.readText(ctx.file.path);
  const colors = text.match(/#[0-9a-fA-F]{6}\b/g) || [];
  host.style.cssText = 'display:flex;flex-wrap:wrap;gap:12px;padding:24px;align-content:flex-start;overflow:auto';
  for (const c of colors) {
    const el = document.createElement('div');
    el.style.cssText = `width:120px;height:120px;border-radius:14px;background:${c};display:flex;align-items:flex-end;padding:8px;color:#fff;font:600 13px system-ui;text-shadow:0 1px 2px #0008`;
    el.textContent = c;
    host.append(el);
  }
  ctx.setStatus(`${colors.length} colors`);
}
