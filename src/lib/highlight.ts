// Lazy syntax highlighting: highlight.js core plus only the grammar a file needs, each in
// its own tiny chunk. Nothing is loaded until a code preview asks for it.
type HLJS = typeof import('highlight.js/lib/core').default;
type Loader = () => Promise<{ default: unknown }>;

const L: Record<string, Loader> = {
  applescript: () => import('highlight.js/lib/languages/applescript'),
  asciidoc: () => import('highlight.js/lib/languages/asciidoc'),
  autohotkey: () => import('highlight.js/lib/languages/autohotkey'),
  awk: () => import('highlight.js/lib/languages/awk'),
  bash: () => import('highlight.js/lib/languages/bash'),
  c: () => import('highlight.js/lib/languages/c'),
  clojure: () => import('highlight.js/lib/languages/clojure'),
  cmake: () => import('highlight.js/lib/languages/cmake'),
  coffeescript: () => import('highlight.js/lib/languages/coffeescript'),
  cpp: () => import('highlight.js/lib/languages/cpp'),
  crystal: () => import('highlight.js/lib/languages/crystal'),
  csharp: () => import('highlight.js/lib/languages/csharp'),
  css: () => import('highlight.js/lib/languages/css'),
  d: () => import('highlight.js/lib/languages/d'),
  dart: () => import('highlight.js/lib/languages/dart'),
  delphi: () => import('highlight.js/lib/languages/delphi'),
  diff: () => import('highlight.js/lib/languages/diff'),
  dockerfile: () => import('highlight.js/lib/languages/dockerfile'),
  dos: () => import('highlight.js/lib/languages/dos'),
  elixir: () => import('highlight.js/lib/languages/elixir'),
  elm: () => import('highlight.js/lib/languages/elm'),
  erb: () => import('highlight.js/lib/languages/erb'),
  erlang: () => import('highlight.js/lib/languages/erlang'),
  fortran: () => import('highlight.js/lib/languages/fortran'),
  fsharp: () => import('highlight.js/lib/languages/fsharp'),
  glsl: () => import('highlight.js/lib/languages/glsl'),
  go: () => import('highlight.js/lib/languages/go'),
  graphql: () => import('highlight.js/lib/languages/graphql'),
  groovy: () => import('highlight.js/lib/languages/groovy'),
  haml: () => import('highlight.js/lib/languages/haml'),
  handlebars: () => import('highlight.js/lib/languages/handlebars'),
  haskell: () => import('highlight.js/lib/languages/haskell'),
  http: () => import('highlight.js/lib/languages/http'),
  ini: () => import('highlight.js/lib/languages/ini'),
  java: () => import('highlight.js/lib/languages/java'),
  javascript: () => import('highlight.js/lib/languages/javascript'),
  json: () => import('highlight.js/lib/languages/json'),
  julia: () => import('highlight.js/lib/languages/julia'),
  kotlin: () => import('highlight.js/lib/languages/kotlin'),
  latex: () => import('highlight.js/lib/languages/latex'),
  less: () => import('highlight.js/lib/languages/less'),
  lisp: () => import('highlight.js/lib/languages/lisp'),
  lua: () => import('highlight.js/lib/languages/lua'),
  makefile: () => import('highlight.js/lib/languages/makefile'),
  markdown: () => import('highlight.js/lib/languages/markdown'),
  nginx: () => import('highlight.js/lib/languages/nginx'),
  nim: () => import('highlight.js/lib/languages/nim'),
  nix: () => import('highlight.js/lib/languages/nix'),
  objectivec: () => import('highlight.js/lib/languages/objectivec'),
  ocaml: () => import('highlight.js/lib/languages/ocaml'),
  perl: () => import('highlight.js/lib/languages/perl'),
  php: () => import('highlight.js/lib/languages/php'),
  powershell: () => import('highlight.js/lib/languages/powershell'),
  processing: () => import('highlight.js/lib/languages/processing'),
  properties: () => import('highlight.js/lib/languages/properties'),
  protobuf: () => import('highlight.js/lib/languages/protobuf'),
  python: () => import('highlight.js/lib/languages/python'),
  r: () => import('highlight.js/lib/languages/r'),
  ruby: () => import('highlight.js/lib/languages/ruby'),
  rust: () => import('highlight.js/lib/languages/rust'),
  scala: () => import('highlight.js/lib/languages/scala'),
  scheme: () => import('highlight.js/lib/languages/scheme'),
  scss: () => import('highlight.js/lib/languages/scss'),
  sql: () => import('highlight.js/lib/languages/sql'),
  stylus: () => import('highlight.js/lib/languages/stylus'),
  swift: () => import('highlight.js/lib/languages/swift'),
  tcl: () => import('highlight.js/lib/languages/tcl'),
  twig: () => import('highlight.js/lib/languages/twig'),
  typescript: () => import('highlight.js/lib/languages/typescript'),
  vbnet: () => import('highlight.js/lib/languages/vbnet'),
  verilog: () => import('highlight.js/lib/languages/verilog'),
  vhdl: () => import('highlight.js/lib/languages/vhdl'),
  vim: () => import('highlight.js/lib/languages/vim'),
  wasm: () => import('highlight.js/lib/languages/wasm'),
  x86asm: () => import('highlight.js/lib/languages/x86asm'),
  xml: () => import('highlight.js/lib/languages/xml'),
  yaml: () => import('highlight.js/lib/languages/yaml'),
};

/** Aliases used by Markdown fences and notebooks. */
const ALIAS: Record<string, string> = {
  js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript', node: 'javascript',
  ts: 'typescript', tsx: 'typescript', py: 'python', python3: 'python', ipython: 'python', ipython3: 'python',
  rb: 'ruby', rs: 'rust', sh: 'bash', shell: 'bash', zsh: 'bash', console: 'bash', shellsession: 'bash',
  yml: 'yaml', html: 'xml', htm: 'xml', svg: 'xml', vue: 'xml', toml: 'ini', 'c++': 'cpp', cc: 'cpp',
  h: 'c', hpp: 'cpp', cs: 'csharp', 'c#': 'csharp', kt: 'kotlin', md: 'markdown', ps1: 'powershell',
  pwsh: 'powershell', golang: 'go', docker: 'dockerfile', patch: 'diff', tex: 'latex', jsonc: 'json',
  json5: 'json', objc: 'objectivec', 'objective-c': 'objectivec', bat: 'dos', cmd: 'dos', make: 'makefile',
  proto: 'protobuf', gql: 'graphql', clj: 'clojure', ex: 'elixir', exs: 'elixir', hs: 'haskell',
  ml: 'ocaml', fs: 'fsharp', jl: 'julia', pl: 'perl', asm: 'x86asm', scheme: 'scheme', r: 'r',
};

/** Languages whose grammars embed others. */
const DEPS: Record<string, string[]> = {
  php: ['xml'], erb: ['xml', 'ruby'], handlebars: ['xml'], twig: ['xml'], haml: ['ruby'], markdown: ['xml'],
  xml: ['css', 'javascript'],
};

let core: Promise<HLJS> | null = null;
const loaded = new Map<string, Promise<boolean>>();

function getCore(): Promise<HLJS> {
  core ??= import('highlight.js/lib/core').then((m) => m.default);
  return core;
}

export function resolveLang(lang: string | null | undefined): string | null {
  if (!lang) return null;
  const l = lang.toLowerCase().trim();
  const name = ALIAS[l] ?? l;
  return L[name] ? name : null;
}

async function ensure(hljs: HLJS, name: string): Promise<boolean> {
  let p = loaded.get(name);
  if (!p) {
    p = (async () => {
      const mod = await L[name]();
      hljs.registerLanguage(name, mod.default as Parameters<HLJS['registerLanguage']>[1]);
      await Promise.all((DEPS[name] ?? []).filter((d) => L[d]).map((d) => ensure(hljs, d)));
      return true;
    })().catch(() => false);
    loaded.set(name, p);
  }
  return p;
}

/** Highlight `code`, returning HTML (escaped by highlight.js), or null if unsupported. */
export async function highlight(code: string, lang: string | null | undefined): Promise<string | null> {
  const name = resolveLang(lang);
  if (!name) return null;
  const hljs = await getCore();
  if (!(await ensure(hljs, name))) return null;
  try {
    return hljs.highlight(code, { language: name, ignoreIllegals: true }).value;
  } catch {
    return null;
  }
}

/** Highlight every `pre > code[class*=language-]` block under `root` (Markdown, notebooks). */
export async function highlightBlocks(root: HTMLElement, signal?: AbortSignal) {
  const blocks = Array.from(root.querySelectorAll<HTMLElement>('pre > code'));
  for (const b of blocks) {
    if (signal?.aborted) return;
    const m = b.className.match(/language-([\w+#-]+)/);
    const text = b.textContent ?? '';
    if (!m || text.length > 200_000) continue;
    const html = await highlight(text, m[1]);
    if (html !== null && !signal?.aborted) {
      b.innerHTML = html;
      b.classList.add('hljs');
    }
  }
}

export const LANG_NAMES: Record<string, string> = {
  javascript: 'JavaScript', typescript: 'TypeScript', python: 'Python', rust: 'Rust', go: 'Go', c: 'C',
  cpp: 'C++', csharp: 'C#', java: 'Java', kotlin: 'Kotlin', swift: 'Swift', ruby: 'Ruby', php: 'PHP',
  bash: 'Shell', powershell: 'PowerShell', dos: 'Batch', sql: 'SQL', xml: 'XML/HTML', css: 'CSS',
  scss: 'SCSS', less: 'Less', yaml: 'YAML', ini: 'Config', json: 'JSON', markdown: 'Markdown',
  dockerfile: 'Dockerfile', makefile: 'Makefile', lua: 'Lua', perl: 'Perl', r: 'R', dart: 'Dart',
  haskell: 'Haskell', elixir: 'Elixir', scala: 'Scala', objectivec: 'Objective-C', diff: 'Diff',
  plaintext: 'Plain text', latex: 'LaTeX', graphql: 'GraphQL', protobuf: 'Protocol Buffers',
  glsl: 'GLSL', cmake: 'CMake', nginx: 'nginx', vbnet: 'Visual Basic', fsharp: 'F#', ocaml: 'OCaml',
  clojure: 'Clojure', erlang: 'Erlang', julia: 'Julia', groovy: 'Groovy', x86asm: 'Assembly',
};
