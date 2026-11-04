/* ==========================================================================
   Inkdown · 语法高亮
   零依赖、离线、不碰 DOM 的代码着色器，给 markdown.js 的围栏代码块用。
   输入是源码字符串，输出是已经转义好的 HTML 片段（token 用 span.tok-*）。

   设计取舍：
   - 每种语言只用「一条大正则 + 分组判断」跑一遍，不写逐字符状态机。
     正则引擎在 C++ 里跑，10k 行的文件也就几十毫秒；状态机在 JS 里逐字符
     走一遍反而慢一个数量级。
   - 转义在前、包标签在后。每个片段先过 escapeHtml 再拼进结果，永远不会
     出现「先拼标签再整体转义」或者「先 replace 再 innerHTML」的逃逸口。
   - 不做 AST，不追求语义完整。高亮是装饰，认错了只是颜色不好看，
     认漏了只是少几个 span，绝不影响源码本身的显示。
   - 已知取舍：Rust 生命周期只认单字符字面量（'a 不会被吃成字符串）、
     JS 正则字面量靠前一个字符猜（除法不会被误判）、CSS 属性靠「名字后面
     跟冒号」猜（元素选择器 a:hover 会被当成属性）。
   ========================================================================== */

/* ========== 基础工具 ========== */

const ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

/** 源码 → 安全 HTML 文本。所有输出都必须经过这里。 */
const escapeHtml = (s) => String(s).replace(/[&<>"']/g, (c) => ESCAPES[c]);

/** 已转义文本 → 带类名的 span。text 必须是原始源码，这里负责转义。 */
const span = (cls, text) => `<span class="${cls}">${escapeHtml(text)}</span>`;

/* ========== 语言别名 ========== */

export const HIGHLIGHT_LANGS = [
  'js', 'ts', 'json', 'html', 'css', 'python',
  'rust', 'bash', 'yaml', 'sql', 'markdown', 'diff',
];

const ALIASES = {
  js: 'js', javascript: 'js', jsx: 'js', mjs: 'js', cjs: 'js', node: 'js', ecmascript: 'js',
  ts: 'ts', typescript: 'ts', tsx: 'ts', mts: 'ts', cts: 'ts',
  json: 'json', jsonc: 'json', json5: 'json',
  html: 'html', htm: 'html', xhtml: 'html', xml: 'html', svg: 'html', vue: 'html', svelte: 'html',
  css: 'css', scss: 'css', sass: 'css', less: 'css', stylus: 'css',
  python: 'python', py: 'python', py3: 'python', python3: 'python', gyp: 'python',
  rust: 'rust', rs: 'rust',
  bash: 'bash', sh: 'bash', shell: 'bash', zsh: 'bash', ksh: 'bash',
  console: 'bash', shellscript: 'bash', terminal: 'bash',
  yaml: 'yaml', yml: 'yaml',
  sql: 'sql', mysql: 'sql', postgres: 'sql', postgresql: 'sql', sqlite: 'sql', plsql: 'sql',
  markdown: 'markdown', md: 'markdown', mdx: 'markdown', mkd: 'markdown',
  diff: 'diff', patch: 'diff', udiff: 'diff',
};

/** 'JavaScript' / 'jsx' / 'py' → 规范名；不认识的返回 ''。 */
export function normalizeLang(raw) {
  let s = String(raw ?? '').trim().toLowerCase();
  if (!s) return '';
  // 兼容 ```language-js / ```{.js} 这类围栏写法
  s = s.replace(/^language-/, '').replace(/^lang-/, '').replace(/^[\s{]*\.?/, '').replace(/\}$/, '');
  return ALIASES[s] || '';
}

/* ========== 扫描引擎 ========== */

/**
 * 规则：{ re: 正则源码字符串, cls: 类名 | (text, match, code, end) => 类名, render?: fn }
 * 所有 re 内部只能用非捕获组 (?:...)，否则分组索引会和规则下标错位。
 * 编译结果按规则数组缓存，模块生命周期内只 new RegExp 一次。
 */
const CACHE = new WeakMap();

function compile(rules, flags) {
  let re = CACHE.get(rules);
  if (!re) {
    re = new RegExp(rules.map((r) => '(' + r.re + ')').join('|'), flags);
    CACHE.set(rules, re);
  }
  return re;
}

/** 单趟交替扫描：命中哪一组就交给哪条规则，剩下原样转义。 */
function scan(code, rules, flags = 'g') {
  const re = compile(rules, flags);
  re.lastIndex = 0;

  let out = '';
  let last = 0;
  let m;

  while ((m = re.exec(code)) !== null) {
    const text = m[0];
    // 零宽匹配（^ 断言等）不推进游标，手动跳过防止死循环
    if (!text) { re.lastIndex++; continue; }

    const start = m.index;
    if (start > last) out += escapeHtml(code.slice(last, start));

    let gi = 1;
    while (gi < m.length && m[gi] === undefined) gi++;
    const rule = rules[gi - 1];

    if (rule.render) {
      out += rule.render(text, code, start);
    } else {
      const cls = typeof rule.cls === 'function'
        ? rule.cls(text, m, code, start + text.length)
        : rule.cls;
      out += cls ? span(cls, text) : escapeHtml(text);
    }

    last = start + text.length;
    re.lastIndex = last;
  }

  if (last < code.length) out += escapeHtml(code.slice(last));
  return out;
}

/* ========== JavaScript / TypeScript ========== */

const JS_KW = new Set([
  'as', 'async', 'await', 'break', 'case', 'catch', 'class', 'const', 'continue',
  'debugger', 'default', 'delete', 'do', 'else', 'enum', 'export', 'extends',
  'finally', 'for', 'from', 'function', 'get', 'if', 'implements', 'import', 'in',
  'instanceof', 'interface', 'keyof', 'let', 'namespace', 'new', 'of', 'private',
  'protected', 'public', 'readonly', 'return', 'satisfies', 'set', 'static',
  'super', 'switch', 'this', 'throw', 'try', 'type', 'typeof', 'var', 'void',
  'while', 'with', 'yield', 'declare', 'abstract', 'infer', 'is', 'asserts', 'override',
]);

const JS_BOOL = new Set(['true', 'false', 'null', 'undefined', 'NaN', 'Infinity']);

function jsIdent(text, m, code, end) {
  if (JS_KW.has(text)) return 'tok-kw';
  if (JS_BOOL.has(text)) return 'tok-bool';
  if (code[m.index - 1] === '.') return 'tok-var'; // 属性访问 obj.prop
  let i = end;
  while (code[i] === ' ' || code[i] === '\t') i++;
  const next = code[i];
  if (next === '(') return 'tok-fn';
  if (next === ':' && code[i + 1] !== ':') return 'tok-key'; // 对象键 / 类型标注
  if (/^[A-Z]/.test(text)) return 'tok-type';
  return '';
}

const JS_RULES = [
  { re: /\/\*[\s\S]*?\*\//.source, cls: 'tok-com' },
  { re: /\/\/[^\n]*/.source, cls: 'tok-com' },
  { re: /`(?:\\[\s\S]|[^\\`])*?`/.source, cls: 'tok-str' },
  { re: /"(?:\\[\s\S]|[^\\"\n])*?"/.source, cls: 'tok-str' },
  { re: /'(?:\\[\s\S]|[^\\'\n])*?'/.source, cls: 'tok-str' },
  // 正则字面量：前一个字符必须是运算符 / 开括号 / return 之类，避开除法
  {
    re: /(?<=[=(,:[!&|?{};+\-*%^~<>]\s*|\b(?:return|typeof|case|in|of|yield|await)\s+)\/(?![*/])(?:\\[\s\S]|\[(?:\\[\s\S]|[^\]\\\n])*\]|[^\/\\\n[])+\/[dgimsuvy]*/.source,
    cls: 'tok-regex',
  },
  { re: /0[xX][\da-fA-F_]+|0[bB][01_]+|0[oO][0-7_]+|\d[\d_]*(?:\.[\d_]+)?(?:[eE][+-]?\d+)?n?/.source, cls: 'tok-num' },
  { re: /=>|===|!==|==|!=|<=|>=|\?\?|\?\.|\.\.\.|&&|\|\||[+\-*%^!?=<>|&]/.source, cls: 'tok-op' },
  { re: /[A-Za-z_$][\w$]*/.source, cls: jsIdent },
];

const highlightJs = (code) => scan(code, JS_RULES);

/* ========== JSON ========== */

const JSON_KW = new Set(['true', 'false', 'null']);

const JSON_RULES = [
  { re: /"(?:\\[\s\S]|[^\\"\n])*?"(?=\s*:)/.source, cls: 'tok-key' },
  { re: /"(?:\\[\s\S]|[^\\"\n])*?"/.source, cls: 'tok-str' },
  { re: /-?\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?/.source, cls: 'tok-num' },
  { re: /[A-Za-z_]\w*/.source, cls: (t) => (JSON_KW.has(t) ? 'tok-bool' : '') },
];

/* ========== HTML ========== */

/**
 * 标签内部再拆一层：< / div / attr / = / "值" / >
 * 只对已经被外层正则圈出来的片段做二次解析，不影响外层的单趟扫描。
 */
const TAG_RE = /^<(\/?)([A-Za-z][\w:.-]*)|([A-Za-z_:@#][-\w:.@#]*)(?:(\s*=\s*)("[^"]*"|'[^']*'|[^\s"'=<>`]+))?|(\/?>)/g;

function renderTag(raw) {
  let out = '';
  let last = 0;
  TAG_RE.lastIndex = 0;
  let m;

  while ((m = TAG_RE.exec(raw)) !== null) {
    if (!m[0]) { TAG_RE.lastIndex++; continue; }

    if (m.index > last) out += escapeHtml(raw.slice(last, m.index));

    if (m[2] !== undefined) {
      // 开合尖括号 + 标签名
      out += span('tok-op', m[1] === '/' ? '</' : '<') + span('tok-tag', m[2]);
    } else if (m[3] !== undefined) {
      out += span('tok-attr', m[3]);
      if (m[4] !== undefined) {
        out += span('tok-op', m[4]);
        out += span('tok-str', m[5]);
      }
    } else {
      out += span('tok-op', m[0]); // > 或 />
    }

    last = m.index + m[0].length;
  }

  if (last < raw.length) out += escapeHtml(raw.slice(last));
  return out;
}

/** <script>/<style>：首标签照常着色，中间交给对应语言，闭合标签再着色。 */
function embedTag(raw, inner) {
  const gt = raw.indexOf('>');
  if (gt < 0) return renderTag(raw);

  const open = raw.slice(0, gt + 1);
  const rest = raw.slice(gt + 1);
  const close = /<\/(?:script|style)\s*>$/i.exec(rest);
  const closeText = close ? close[0] : '';
  const body = closeText ? rest.slice(0, rest.length - closeText.length) : rest;

  return renderTag(open) + inner(body) + (closeText ? renderTag(closeText) : '');
}

const HTML_RULES = [
  { re: /<!--[\s\S]*?-->/.source, cls: 'tok-com' },
  { re: /<!\[CDATA\[[\s\S]*?\]\]>/.source, cls: 'tok-com' },
  { re: /<!DOCTYPE[^>]*>/i.source, cls: 'tok-tag' },
  { re: /<script\b[^>]*>[\s\S]*?<\/script\s*>/i.source, render: (raw) => embedTag(raw, highlightJs) },
  { re: /<style\b[^>]*>[\s\S]*?<\/style\s*>/i.source, render: (raw) => embedTag(raw, highlightCss) },
  // 标签体用 [^>]* 保线性；带 > 的引号属性会提前截断，属可接受的边角
  { re: /<\/?[A-Za-z][\w:.-]*[^>]*>/.source, render: (raw) => renderTag(raw) },
];

/* ========== CSS ========== */

const CSS_RULES = [
  { re: /\/\*[\s\S]*?\*\//.source, cls: 'tok-com' },
  { re: /"(?:\\[\s\S]|[^\\"\n])*?"|'(?:\\[\s\S]|[^\\'\n])*?'/.source, cls: 'tok-str' },
  { re: /@[-\w]+/.source, cls: 'tok-kw' },
  { re: /!important\b/.source, cls: 'tok-kw' },
  // 类 / id 选择器：后面（且没跨过 ; 或 }）能遇到 { 才算选择器，#fff 这类颜色不算
  { re: /[.#][-\w]+(?=[^;{}]*\{)/.source, cls: 'tok-type' },
  { re: /[-\w]+(?=\s*\()/.source, cls: 'tok-fn' },
  { re: /[-\w]+(?=\s*:)/.source, cls: 'tok-key' },
  { re: /#[0-9a-f]{3,8}\b/.source, cls: 'tok-num' },
  { re: /\b\d+(?:\.\d+)?(?:px|em|rem|vh|vw|vmin|vmax|ms|s|deg|rad|turn|fr|pt|pc|ch|ex|dpi|%)?\b/.source, cls: 'tok-num' },
  { re: /\b(?:none|auto|inherit|initial|unset|revert|flex|grid|block|inline|inline-block|absolute|relative|fixed|sticky|static|bold|italic|transparent|currentcolor|hidden|visible|center|nowrap|pointer|uppercase|lowercase)\b/.source, cls: 'tok-kw' },
  { re: /[{}();:,]/.source, cls: 'tok-op' },
];

const highlightCss = (code) => scan(code, CSS_RULES, 'gi');

/* ========== Python ========== */

const PY_KW = new Set([
  'and', 'as', 'assert', 'async', 'await', 'break', 'class', 'continue', 'def',
  'del', 'elif', 'else', 'except', 'finally', 'for', 'from', 'global', 'if',
  'import', 'in', 'is', 'lambda', 'nonlocal', 'not', 'or', 'pass', 'raise',
  'return', 'try', 'while', 'with', 'yield', 'match', 'case', 'print',
]);

const PY_BOOL = new Set(['True', 'False', 'None']);

function pyIdent(text, m, code, end) {
  if (PY_KW.has(text)) return 'tok-kw';
  if (PY_BOOL.has(text)) return 'tok-bool';
  if (text === 'self' || text === 'cls') return 'tok-var';
  let i = end;
  while (code[i] === ' ' || code[i] === '\t') i++;
  if (code[i] === '(') return 'tok-fn';
  if (/^[A-Z]/.test(text)) return 'tok-type';
  return '';
}

const PY_RULES = [
  { re: /#[^\n]*/.source, cls: 'tok-com' },
  // 三引号必须先于单引号，否则 """ 会被拆成两个空字符串
  { re: /[rRbBuUfF]{0,2}"""[\s\S]*?"""/.source, cls: 'tok-str' },
  { re: /[rRbBuUfF]{0,2}'''[\s\S]*?'''/.source, cls: 'tok-str' },
  { re: /[rRbBuUfF]{0,2}"(?:\\[\s\S]|[^\\"\n])*?"/.source, cls: 'tok-str' },
  { re: /[rRbBuUfF]{0,2}'(?:\\[\s\S]|[^\\'\n])*?'/.source, cls: 'tok-str' },
  { re: /@[\w.]+/.source, cls: 'tok-fn' },
  { re: /\b\d[\d_]*(?:\.[\d_]*)?(?:[eE][+-]?\d+)?j?\b/.source, cls: 'tok-num' },
  { re: /:=|->|==|!=|<=|>=|\/\/|\*\*|[-+*/%=<>@&|^~]/.source, cls: 'tok-op' },
  { re: /[A-Za-z_]\w*/.source, cls: pyIdent },
];

/* ========== Rust ========== */

const RS_KW = new Set([
  'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else',
  'enum', 'extern', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop', 'match',
  'mod', 'move', 'mut', 'pub', 'ref', 'return', 'self', 'Self', 'static',
  'struct', 'super', 'trait', 'type', 'unsafe', 'use', 'where', 'while', 'yield',
  'box', 'macro_rules',
]);

const RS_BOOL = new Set(['true', 'false']);

function rsIdent(text, m, code, end) {
  if (RS_KW.has(text)) return 'tok-kw';
  if (RS_BOOL.has(text)) return 'tok-bool';
  let i = end;
  while (code[i] === ' ' || code[i] === '\t') i++;
  if (code[i] === '(') return 'tok-fn';
  if (code[i] === '!') return 'tok-fn'; // 宏调用
  if (/^[A-Z]/.test(text)) return 'tok-type';
  return '';
}

const RS_RULES = [
  { re: /\/\*[\s\S]*?\*\//.source, cls: 'tok-com' },
  { re: /\/\/[^\n]*/.source, cls: 'tok-com' },
  { re: /r#*"[\s\S]*?"#*/.source, cls: 'tok-str' },
  { re: /b?"(?:\\[\s\S]|[^\\"])*?"/.source, cls: 'tok-str' },
  // 只认单字符字面量，这样 'a 生命周期不会把后面整段吞成字符串
  { re: /b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.)|[^'\\\n])'/.source, cls: 'tok-str' },
  { re: /'[A-Za-z_]\w*/.source, cls: 'tok-type' },
  { re: /#!?\[[^\]\n]*\]/.source, cls: 'tok-com' },
  { re: /[a-z_]\w*!/.source, cls: 'tok-fn' },
  { re: /\b(?:0[xX][\da-fA-F_]+|0[bB][01_]+|0[oO][0-7_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)(?:[iu](?:8|16|32|64|128|size)|f(?:32|64))?\b/.source, cls: 'tok-num' },
  { re: /::|->|=>|\.\.=|\.\.|==|!=|<=|>=|&&|\|\||[-+*/%=<>!&|^~?]/.source, cls: 'tok-op' },
  { re: /[A-Za-z_]\w*/.source, cls: rsIdent },
];

/* ========== Bash ========== */

const BASH_RULES = [
  { re: /(?:^|(?<=\s))#[^\n]*/.source, cls: 'tok-com' },
  { re: /"(?:\\[\s\S]|[^\\"])*?"/.source, cls: 'tok-str' },
  { re: /'[^']*?'/.source, cls: 'tok-str' },
  { re: /\$\{[^}\n]*\}|\$[A-Za-z_]\w*|\$[@#?$!*\-0-9]/.source, cls: 'tok-var' },
  { re: /\b(?:if|then|else|elif|fi|for|in|do|done|while|until|case|esac|function|select|return|local|export|declare|readonly|unset|shift|trap|set|source|exit)\b/.source, cls: 'tok-kw' },
  { re: /\b(?:echo|printf|read|cd|pwd|test|cat|grep|sed|awk|curl|wget|git|npm|node|python|chmod|mkdir|rm|cp|mv)\b/.source, cls: 'tok-fn' },
  { re: /(?:^|(?<=\s))--?[A-Za-z][\w-]*/.source, cls: 'tok-attr' },
  { re: /\b\d+\b/.source, cls: 'tok-num' },
  { re: /\|\||&&|[|&;<>()[\]{}=]/.source, cls: 'tok-op' },
];

/* ========== YAML ========== */

const YAML_RULES = [
  { re: /(?:^|(?<=\s))#[^\n]*/.source, cls: 'tok-com' },
  { re: /^---[ \t]*$|^\.\.\.[ \t]*$/.source, cls: 'tok-kw' },
  { re: /^[ \t]*(?:-[ \t]+)?[A-Za-z_$][\w.$/-]*(?=[ \t]*:)/.source, cls: 'tok-key' },
  { re: /"(?:\\[\s\S]|[^\\"\n])*?"|'(?:[^'\n]|'')*'/.source, cls: 'tok-str' },
  { re: /[&*][\w-]+/.source, cls: 'tok-var' },
  { re: /![\w:/-]+/.source, cls: 'tok-type' },
  { re: /\b(?:true|false|null|yes|no|on|off)\b|~/.source, cls: 'tok-bool' },
  { re: /\b-?\d+(?:\.\d+)?\b/.source, cls: 'tok-num' },
  { re: /^[ \t]*-[ \t]/.source, cls: 'tok-op' },
  { re: /[\[\]{},]/.source, cls: 'tok-op' },
];

/* ========== SQL ========== */

const SQL_RULES = [
  { re: /--[^\n]*/.source, cls: 'tok-com' },
  { re: /\/\*[\s\S]*?\*\//.source, cls: 'tok-com' },
  { re: /'(?:[^'\n]|'')*'/.source, cls: 'tok-str' },
  { re: /"(?:[^"\n]|"")*"/.source, cls: 'tok-str' },
  { re: /`[^`\n]*`/.source, cls: 'tok-attr' },
  {
    re: /\b(?:select|from|where|group|order|by|having|limit|offset|insert|into|values|update|set|delete|create|table|view|index|alter|add|drop|truncate|join|left|right|inner|outer|full|cross|on|as|and|or|not|in|is|like|between|exists|distinct|union|all|intersect|except|case|when|then|else|end|if|null|primary|foreign|key|references|unique|default|constraint|check|database|begin|commit|rollback|with|asc|desc|top)\b/.source,
    cls: 'tok-kw',
  },
  { re: /\b(?:true|false)\b/.source, cls: 'tok-bool' },
  { re: /\b[A-Za-z_]\w*(?=\s*\()/.source, cls: 'tok-fn' },
  { re: /\b\d+(?:\.\d+)?\b/.source, cls: 'tok-num' },
  { re: /@\w+|:\w+|\$\d+/.source, cls: 'tok-var' },
  { re: /<>|!=|<=|>=|\|\||[=<>+\-*/%]/.source, cls: 'tok-op' },
];

/* ========== Markdown ========== */

const MD_RULES = [
  { re: /```[\s\S]*?```|~~~[\s\S]*?~~~/.source, cls: 'tok-com' },
  { re: /^#{1,6}[^\n]*/.source, cls: 'tok-kw' },
  { re: /^[ \t]*(?:[-*_][ \t]*){3,}$/.source, cls: 'tok-com' },
  { re: /^[ \t]*>/.source, cls: 'tok-op' },
  { re: /^[ \t]*(?:[-*+]|\d+[.)])(?=\s)/.source, cls: 'tok-op' },
  { re: /`[^`\n]+`/.source, cls: 'tok-str' },
  { re: /\*\*[^*\n]+\*\*|__[^_\n]+__/.source, cls: 'tok-kw' },
  { re: /\*[^*\n]+\*|_[^_\n]+_/.source, cls: 'tok-type' },
  { re: /!?\[[^\]\n]*\]\([^)\n]*\)/.source, cls: 'tok-fn' },
  { re: /<(?:https?:\/\/|mailto:)[^>\s]+>/.source, cls: 'tok-fn' },
  { re: /~~[^~\n]+~~/.source, cls: 'tok-com' },
];

/* ========== Diff ========== */

/** 按行首符号着色。逐行判断本来就是这个语言的语义，不需要正则交替。 */
function renderDiff(code) {
  const lines = String(code).split('\n');
  let out = '';

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let cls = '';

    if (/^(?:\+\+\+|---) /.test(line)) cls = 'tok-com';          // 文件头
    else if (/^(?:diff |index |new file|deleted file|similarity |rename )/.test(line)) cls = 'tok-com';
    else if (/^@@/.test(line)) cls = 'tok-type';                 // hunk 头
    else if (/^\+/.test(line)) cls = 'tok-add';
    else if (/^-/.test(line)) cls = 'tok-del';
    else if (/^\\/.test(line)) cls = 'tok-com';

    out += cls ? span(cls, line) : escapeHtml(line);
    if (i < lines.length - 1) out += '\n';
  }

  return out;
}

/* ========== 出口 ========== */

const RENDERERS = {
  js: highlightJs,
  ts: highlightJs,
  json: (code) => scan(code, JSON_RULES),
  html: (code) => scan(code, HTML_RULES, 'gi'),
  css: highlightCss,
  python: (code) => scan(code, PY_RULES),
  rust: (code) => scan(code, RS_RULES),
  bash: (code) => scan(code, BASH_RULES, 'gm'),
  yaml: (code) => scan(code, YAML_RULES, 'gim'),
  sql: (code) => scan(code, SQL_RULES, 'gi'),
  markdown: (code) => scan(code, MD_RULES, 'gm'),
  diff: renderDiff,
};

/**
 * 源码 → 高亮 HTML。
 * 未知语言 / 空 lang 走快速路径：只转义，不加任何 span。
 */
export function highlight(code, lang) {
  const text = String(code ?? '');
  const key = normalizeLang(lang);
  const render = key ? RENDERERS[key] : null;
  return render ? render(text) : escapeHtml(text);
}
