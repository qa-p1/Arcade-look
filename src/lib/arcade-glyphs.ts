// Copied from Arcade-link/assets/glyphs; trusted app glyphs only.
import { svg } from "./dom";

const glyphs: Record<string, string> = {
  "arcade.box": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linejoin=\"round\"><path d=\"M8 1.75 14 4.75v6.5L8 14.25 2 11.25v-6.5z\"/><path d=\"M2 4.75 8 7.75l6-3M8 7.75v6.5\"/></svg>",
  "arcade.clipboard": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linejoin=\"round\"><path d=\"M5.5 2.75H4a1.25 1.25 0 0 0-1.25 1.25v9A1.25 1.25 0 0 0 4 14.25h8A1.25 1.25 0 0 0 13.25 13V4A1.25 1.25 0 0 0 12 2.75h-1.5\"/><rect x=\"5.5\" y=\"1.5\" width=\"5\" height=\"2.5\" rx=\".75\"/><path d=\"M5.5 8h5M5.5 10.75h3.5\" stroke-linecap=\"round\"/></svg>",
  "arcade.find": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"7\" cy=\"7\" r=\"4.75\"/><path d=\"M10.5 10.5 14.25 14.25\"/></svg>",
  "arcade.lens": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\"><path d=\"M1.75 5V2.75a1 1 0 0 1 1-1H5M11 1.75h2.25a1 1 0 0 1 1 1V5M14.25 11v2.25a1 1 0 0 1-1 1H11M5 14.25H2.75a1 1 0 0 1-1-1V11\"/><circle cx=\"8\" cy=\"8\" r=\"2.25\"/></svg>",
  "arcade.look": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linejoin=\"round\"><path d=\"M1.25 8S3.75 3.25 8 3.25 14.75 8 14.75 8 12.25 12.75 8 12.75 1.25 8 1.25 8z\"/><circle cx=\"8\" cy=\"8\" r=\"2\"/></svg>",
  "arcade.shelf": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M1.75 10.75h12.5v3.5H1.75z\"/><rect x=\"3.25\" y=\"4.75\" width=\"3.5\" height=\"4\" rx=\".75\"/><rect x=\"8.75\" y=\"2\" width=\"4\" height=\"6.75\" rx=\".75\"/></svg>",
  "arcade.tools": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M10.5 1.9a3.5 3.5 0 0 0-3.9 4.7L1.9 11.3a1.4 1.4 0 0 0 2 2l4.7-4.7a3.5 3.5 0 0 0 4.7-3.9l-2 2-1.9-.4-.4-1.9z\"/></svg>",
  "arcade.wheel": "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\"><circle cx=\"8\" cy=\"8\" r=\"6.25\"/><circle cx=\"8\" cy=\"8\" r=\"1.75\"/><path d=\"M8 1.75v4.5M8 9.75v4.5M1.75 8h4.5M9.75 8h4.5\"/></svg>"
};

export function arcadeGlyph(app: string): SVGElement {
  const el = svg(glyphs[app] ?? glyphs["arcade.look"]);
  el.setAttribute("width", "16");
  el.setAttribute("height", "16");
  el.setAttribute("aria-hidden", "true");
  return el;
}
