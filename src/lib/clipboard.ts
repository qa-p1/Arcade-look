// One copy path, including older WebKit builds without navigator.clipboard.
import { h } from './dom';
export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const input = h('textarea', { value: text, style: 'position:fixed;opacity:0' });
    document.body.append(input);
    input.select();
    const copied = document.execCommand('copy');
    input.remove();
    if (!copied) throw new Error("Couldn't copy the text. Try again.");
  }
}
