//! 零依赖 Markdown 渲染器（含双链、图片嵌入、交互式任务清单、代码高亮与行号映射）。

pub mod highlight;

use highlight::{escape_html, highlight_code, normalize_lang};

pub fn safe_url(url: &str, image: bool) -> String {
    let u = url.trim();
    let lc = u.to_ascii_lowercase();
    if lc.starts_with("javascript:") || lc.starts_with("vbscript:") || lc.starts_with("file:") {
        return "#".to_string();
    }
    if lc.starts_with("data:") && !(image && lc.starts_with("data:image/")) {
        return "#".to_string();
    }
    u.to_string()
}

/* ---------------- 行内解析 ---------------- */

fn render_inline(text: &str) -> String {
    // 先把行内代码 `...` 抽取出来，避免里面的 * _ [[ 被误解析
    const TOKEN: char = '\u{0000}';
    let mut codes: Vec<String> = Vec::new();
    let mut masked = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            let mut ticks = 0usize;
            while i + ticks < chars.len() && chars[i + ticks] == '`' {
                ticks += 1;
            }
            let mut j = i + ticks;
            let mut close_start = None;
            while j < chars.len() && chars[j] != '\n' {
                if chars[j] == '`' {
                    let mut run = 0usize;
                    while j + run < chars.len() && chars[j + run] == '`' {
                        run += 1;
                    }
                    if run == ticks {
                        close_start = Some(j);
                        break;
                    }
                    j += run;
                } else {
                    j += 1;
                }
            }
            if let Some(end) = close_start {
                let mut code: String = chars[i + ticks..end].iter().collect();
                // CommonMark：若首尾各有空格且内部非纯空格，剥离一对空格（兼容 `` `code` `` 写法）
                if code.len() >= 2
                    && code.starts_with(' ')
                    && code.ends_with(' ')
                    && code.chars().any(|c| c != ' ')
                {
                    code = code[1..code.len() - 1].to_string();
                }
                let idx = codes.len();
                codes.push(code);
                masked.push(TOKEN);
                masked.push_str(&idx.to_string());
                masked.push(TOKEN);
                i = end + ticks;
                continue;
            } else {
                // 未闭合的连续反引号整体原样保留，避免内部拆分错误配对
                for _ in 0..ticks {
                    masked.push('`');
                }
                i += ticks;
                continue;
            }
        }
        masked.push(chars[i]);
        i += 1;
    }

    let mut s = escape_html(&masked);

    // 1. Obsidian 嵌入图片：![[image.png]] 或 ![[image.png|300]]
    s = replace_double_brackets(&s, true);

    // 2. 标准 Markdown 图片：![alt](src)
    s = replace_md_links_or_images(&s, true);

    // 3. Obsidian 双链：[[note]] / [[note|别名]] / [[note#标题]]
    s = replace_double_brackets(&s, false);

    // 4. 标准 Markdown 链接：[label](href)
    s = replace_md_links_or_images(&s, false);

    // 5. 强调与删除线
    s = replace_delimited(&s, "**", "strong");
    s = replace_delimited(&s, "__", "strong");
    s = replace_delimited(&s, "~~", "del");
    // Obsidian 的 ==高亮==：语义是「标记」，配色里单独占一档
    s = replace_delimited(&s, "==", "mark");
    s = replace_single_delimited(&s, '*', "em");
    s = replace_single_delimited(&s, '_', "em");

    // 6. 还原行内代码
    let mut restored = String::with_capacity(s.len());
    let mut rest_idx = 0;
    while let Some(rel) = s[rest_idx..].find(TOKEN) {
        let start = rest_idx + rel;
        restored.push_str(&s[rest_idx..start]);
        let after = start + TOKEN.len_utf8();
        if let Some(rel_end) = s[after..].find(TOKEN) {
            let num_str = &s[after..after + rel_end];
            if let Ok(idx) = num_str.parse::<usize>() {
                if let Some(code) = codes.get(idx) {
                    restored.push_str("<code>");
                    restored.push_str(&escape_html(code));
                    restored.push_str("</code>");
                    rest_idx = after + rel_end + TOKEN.len_utf8();
                    continue;
                }
            }
        }
        restored.push(TOKEN);
        rest_idx = after;
    }
    restored.push_str(&s[rest_idx..]);
    restored
}

fn replace_double_brackets(input: &str, embed: bool) -> String {
    let prefix = if embed { "![[" } else { "[[" };
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while let Some(rel) = input[i..].find(prefix) {
        let start = i + rel;
        // 避免把 ![[...]] 在非 embed 阶段再次处理
        if !embed && start > 0 && input.as_bytes()[start - 1] == b'!' {
            out.push_str(&input[i..start + 2]);
            i = start + 2;
            continue;
        }
        out.push_str(&input[i..start]);
        let content_start = start + prefix.len();
        let Some(rel_end) = input[content_start..].find("]]") else {
            out.push_str(&input[start..]);
            return out;
        };
        let end = content_start + rel_end;
        let inner = &input[content_start..end];
        i = end + 2;

        if inner.contains('\n') || inner.contains('[') {
            out.push_str(&input[start..i]);
            continue;
        }

        if embed {
            let mut parts = inner.splitn(2, '|');
            let target = parts.next().unwrap_or("").trim();
            let meta = parts.next().map(str::trim).unwrap_or("");
            let width_style = if !meta.is_empty() && meta.chars().all(|c| c.is_ascii_digit()) {
                format!(" style=\"max-width:{meta}px\"")
            } else {
                String::new()
            };
            let is_remote = target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("data:");
            let src_attr = if is_remote { target } else { "" };
            out.push_str(&format!(
                "<img class=\"ink-img\" src=\"{src_attr}\" data-vault-src=\"{target}\" alt=\"{target}\" loading=\"lazy\"{width_style}>"
            ));
        } else {
            let mut parts = inner.splitn(2, '|');
            let target = parts.next().unwrap_or("").trim();
            let alias = parts.next().map(str::trim).filter(|a| !a.is_empty());
            let label = alias.unwrap_or(target);
            out.push_str(&format!(
                "<a class=\"ink-wikilink\" href=\"#wikilink\" data-target=\"{target}\" title=\"打开: {target}\">{label}</a>"
            ));
        }
    }
    out.push_str(&input[i..]);
    out
}

fn replace_md_links_or_images(input: &str, image: bool) -> String {
    let prefix = if image { "![" } else { "[" };
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while let Some(rel) = input[i..].find(prefix) {
        let start = i + rel;
        if !image && start > 0 && input.as_bytes()[start - 1] == b'!' {
            out.push_str(&input[i..start + 1]);
            i = start + 1;
            continue;
        }
        out.push_str(&input[i..start]);
        let label_start = start + prefix.len();
        let Some(rel_mid) = input[label_start..].find("](") else {
            out.push_str(&input[start..]);
            return out;
        };
        let label_end = label_start + rel_mid;
        let label = &input[label_start..label_end];
        let url_start = label_end + 2;
        let Some(rel_close) = input[url_start..].find(')') else {
            out.push_str(&input[start..]);
            return out;
        };
        let url_end = url_start + rel_close;
        let raw_url = input[url_start..url_end].trim();

        if label.contains('\n') || raw_url.contains('\n') || raw_url.contains(' ') {
            out.push_str(&input[start..url_end + 1]);
            i = url_end + 1;
            continue;
        }

        i = url_end + 1;
        if image {
            let is_remote = raw_url.starts_with("http://")
                || raw_url.starts_with("https://")
                || raw_url.starts_with("data:");
            let src_attr = if is_remote {
                safe_url(raw_url, true)
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<img class=\"ink-img\" src=\"{src_attr}\" data-vault-src=\"{raw_url}\" alt=\"{label}\" loading=\"lazy\">"
            ));
        } else {
            let url = safe_url(raw_url, false);
            let external = url.starts_with("http://") || url.starts_with("https://");
            let ext_attr = if external {
                " target=\"_blank\" rel=\"noreferrer\""
            } else {
                ""
            };
            out.push_str(&format!("<a href=\"{url}\"{ext_attr}>{label}</a>"));
        }
    }
    out.push_str(&input[i..]);
    out
}

fn replace_delimited(input: &str, delim: &str, tag: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while let Some(rel) = input[i..].find(delim) {
        let start = i + rel;
        let inner_start = start + delim.len();
        let after = &input[inner_start..];
        if after.starts_with(char::is_whitespace) {
            out.push_str(&input[i..inner_start]);
            i = inner_start;
            continue;
        }
        let Some(rel_end) = after.find(delim) else {
            out.push_str(&input[i..]);
            return out;
        };
        let inner = &after[..rel_end];
        if inner.is_empty() || inner.ends_with(char::is_whitespace) || inner.contains('\n') {
            out.push_str(&input[i..inner_start]);
            i = inner_start;
            continue;
        }
        out.push_str(&input[i..start]);
        out.push_str(&format!("<{tag}>{inner}</{tag}>"));
        i = inner_start + rel_end + delim.len();
    }
    out.push_str(&input[i..]);
    out
}

fn replace_single_delimited(input: &str, marker: char, tag: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        if chars[i] == marker {
            // 跳过 HTML 属性值内的下划线/星号（如 data-vault-src="a_b.png"）
            let prev_ok = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == marker);
            let next_ok = i + 1 < n && !chars[i + 1].is_whitespace() && chars[i + 1] != marker;
            if prev_ok && next_ok {
                let mut end = i + 1;
                let mut found = None;
                while end < n && chars[end] != '\n' {
                    if chars[end] == marker
                        && !chars[end - 1].is_whitespace()
                        && (end + 1 >= n || !chars[end + 1].is_alphanumeric())
                    {
                        found = Some(end);
                        break;
                    }
                    end += 1;
                }
                if let Some(close) = found {
                    let inner: String = chars[i + 1..close].iter().collect();
                    if !inner.contains('<') && !inner.contains('>') {
                        out.push_str(&format!("<{tag}>{inner}</{tag}>"));
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/* ---------------- 块级解析 ---------------- */

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

fn parse_fence_open(line: &str) -> Option<(char, usize, String)> {
    let trimmed = line.trim_start();
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let count = trimmed.chars().take_while(|&c| c == ch).count();
    if count < 3 {
        return None;
    }
    let info = trimmed[count..].trim();
    if ch == '`' && info.contains('`') {
        return None;
    }
    let lang = info.split_whitespace().next().unwrap_or("").to_string();
    Some((ch, count, lang))
}

fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let body = rest
        .trim()
        .trim_end_matches('#')
        .trim_end_matches(char::is_whitespace);
    Some((level, body.to_string()))
}

fn is_rule(line: &str) -> bool {
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    let ch = match t.chars().next() {
        Some(c @ ('*' | '-' | '_')) => c,
        _ => return false,
    };
    let count = t.chars().filter(|&c| c == ch).count();
    count >= 3 && t.chars().all(|c| c == ch || c.is_whitespace())
}

fn strip_quote(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if line.len() - trimmed.len() > 3 {
        return None;
    }
    let rest = trimmed.strip_prefix('>')?;
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}

struct ListMatch {
    indent: usize,
    ordered: bool,
    text: String,
}

fn parse_list_item(line: &str) -> Option<ListMatch> {
    let mut indent = 0usize;
    let mut rest_idx = 0usize;
    for (idx, ch) in line.char_indices() {
        match ch {
            ' ' => {
                indent += 1;
                rest_idx = idx + 1;
            }
            '\t' => {
                indent += 4;
                rest_idx = idx + 1;
            }
            _ => break,
        }
    }
    let rest = &line[rest_idx..];
    if let Some(after) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        return Some(ListMatch {
            indent,
            ordered: false,
            text: after.to_string(),
        });
    }
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let after_digits = &rest[digits..];
        if let Some(after) = after_digits
            .strip_prefix(". ")
            .or_else(|| after_digits.strip_prefix(") "))
        {
            return Some(ListMatch {
                indent,
                ordered: true,
                text: after.to_string(),
            });
        }
    }
    None
}

fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    t.contains('-')
        && t.chars()
            .all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn is_table_start(lines: &[&str], i: usize) -> bool {
    if i + 1 >= lines.len() {
        return false;
    }
    lines[i].contains('|') && is_table_separator(lines[i + 1])
}

fn split_table_row(line: &str) -> Vec<String> {
    let t = line.trim().trim_start_matches('|').trim_end_matches('|');
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn render_table(lines: &[&str], start: usize) -> (String, usize) {
    let head = split_table_row(lines[start]);
    let align: Vec<&'static str> = split_table_row(lines[start + 1])
        .into_iter()
        .map(|cell| {
            let l = cell.starts_with(':');
            let r = cell.ends_with(':');
            match (l, r) {
                (true, true) => "center",
                (false, true) => "right",
                (true, false) => "left",
                _ => "",
            }
        })
        .collect();

    let mut i = start + 2;
    let mut rows = Vec::new();
    while i < lines.len() && !is_blank(lines[i]) && lines[i].contains('|') {
        rows.push(split_table_row(lines[i]));
        i += 1;
    }

    let cell_html = |tag: &str, val: &str, idx: usize| -> String {
        let a = align.get(idx).copied().unwrap_or("");
        let style = if a.is_empty() {
            String::new()
        } else {
            format!(" style=\"text-align:{a}\"")
        };
        format!("<{tag}{style}>{}</{tag}>", render_inline(val))
    };

    let thead: String = head
        .iter()
        .enumerate()
        .map(|(k, c)| cell_html("th", c, k))
        .collect();
    let tbody: String = rows
        .iter()
        .map(|r| {
            let cols: String = (0..head.len())
                .map(|k| cell_html("td", r.get(k).map(String::as_str).unwrap_or(""), k))
                .collect();
            format!("<tr>{cols}</tr>")
        })
        .collect();

    let html = format!(
        "<div class=\"md-table\"><table><thead><tr>{thead}</tr></thead>{}</table></div>",
        if tbody.is_empty() {
            String::new()
        } else {
            format!("<tbody>{tbody}</tbody>")
        }
    );
    (html, i)
}

#[derive(Clone)]
struct ListItemNode {
    indent: usize,
    ordered: bool,
    text: String,
    line: usize,
    children: Vec<ListItemNode>,
}

fn render_list(lines: &[&str], start: usize, offset: usize) -> (String, usize) {
    let mut items_flat: Vec<(usize, bool, String, usize)> = Vec::new();
    let mut base_indent: Option<usize> = None;
    let mut i = start;

    while i < lines.len() {
        let line = lines[i];
        if let Some(m) = parse_list_item(line) {
            let base = *base_indent.get_or_insert(m.indent);
            if m.indent < base {
                break;
            }
            // 同级切换列表类型时断开
            if m.indent == base {
                if let Some((_, first_ord, _, _)) = items_flat.first() {
                    if *first_ord != m.ordered {
                        break;
                    }
                }
            }
            items_flat.push((m.indent, m.ordered, m.text, offset + i));
            i += 1;
            continue;
        }
        if !items_flat.is_empty()
            && !is_blank(line)
            && line.starts_with("  ")
            && !is_table_start(lines, i)
        {
            if let Some(last) = items_flat.last_mut() {
                last.2.push('\n');
                last.2.push_str(line.trim());
            }
            i += 1;
            continue;
        }
        if is_blank(line) {
            let mut j = i + 1;
            while j < lines.len() && is_blank(lines[j]) {
                j += 1;
            }
            if j < lines.len() {
                if let Some(next_m) = parse_list_item(lines[j]) {
                    if next_m.indent >= base_indent.unwrap_or(0) {
                        i = j;
                        continue;
                    }
                }
            }
        }
        break;
    }

    // 建树
    fn build_tree(items: &[(usize, bool, String, usize)]) -> Vec<ListItemNode> {
        let mut roots: Vec<ListItemNode> = Vec::new();
        for &(indent, ordered, ref text, line) in items {
            let node = ListItemNode {
                indent,
                ordered,
                text: text.clone(),
                line,
                children: Vec::new(),
            };
            insert_node(&mut roots, node);
        }
        roots
    }

    fn insert_node(list: &mut Vec<ListItemNode>, node: ListItemNode) {
        if let Some(last) = list.last_mut() {
            if node.indent > last.indent {
                insert_node(&mut last.children, node);
                return;
            }
        }
        list.push(node);
    }

    fn render_nodes(nodes: &[ListItemNode]) -> String {
        if nodes.is_empty() {
            return String::new();
        }
        let tag = if nodes[0].ordered { "ol" } else { "ul" };
        let inner: String = nodes.iter().map(render_node).collect();
        format!("<{tag}>{inner}</{tag}>")
    }

    fn render_node(item: &ListItemNode) -> String {
        let task = item
            .text
            .strip_prefix("[ ] ")
            .map(|rest| (false, rest))
            .or_else(|| item.text.strip_prefix("[x] ").map(|rest| (true, rest)))
            .or_else(|| item.text.strip_prefix("[X] ").map(|rest| (true, rest)));

        let (cls, body) = if let Some((checked, rest)) = task {
            let chk = if checked { " checked" } else { "" };
            (
                " class=\"md-li-task\"",
                format!(
                    "<label class=\"md-task\"><input type=\"checkbox\"{chk} data-task-line=\"{}\"><span>{}</span></label>",
                    item.line,
                    render_inline(rest)
                ),
            )
        } else {
            ("", render_inline(&item.text).replace('\n', "<br>"))
        };

        let kids = render_nodes(&item.children);
        format!("<li{cls}>{body}{kids}</li>")
    }

    let roots = build_tree(&items_flat);
    (render_nodes(&roots), i)
}

fn starts_block(lines: &[&str], i: usize) -> bool {
    let line = lines[i];
    parse_fence_open(line).is_some()
        || parse_heading(line).is_some()
        || is_rule(line)
        || strip_quote(line).is_some()
        || parse_list_item(line).is_some()
        || is_table_start(lines, i)
}

fn render_blocks(lines: &[&str], offset: usize) -> String {
    let mut out = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if is_blank(line) {
            i += 1;
            continue;
        }

        if let Some((marker, open_len, lang)) = parse_fence_open(line) {
            let mut buf = Vec::new();
            i += 1;
            while i < lines.len() {
                let t = lines[i].trim();
                let is_close = t.len() >= open_len
                    && t.chars().all(|c| c == marker);
                if is_close {
                    i += 1;
                    break;
                }
                buf.push(lines[i]);
                i += 1;
            }
            let raw_code = buf.join("\n");
            let label = if lang.is_empty() {
                "TEXT".to_string()
            } else {
                escape_html(&lang)
            };
            let norm = normalize_lang(&lang);
            let data_lang = if norm.is_empty() {
                escape_html(&lang)
            } else {
                norm.to_string()
            };
            let highlighted = highlight_code(&raw_code, &lang);
            out.push(format!(
                "<div class=\"md-code\"><div class=\"md-code__header\"><span class=\"md-code__lang\">{label}</span><button type=\"button\" class=\"md-code__copy\" data-copy-code=\"1\" aria-label=\"复制代码\" title=\"复制代码\">复制</button></div><pre><code data-lang=\"{data_lang}\">{highlighted}</code></pre></div>"
            ));
            continue;
        }

        if let Some((level, body)) = parse_heading(line) {
            out.push(format!(
                "<h{level} data-line=\"{}\">{}</h{level}>",
                i + offset,
                render_inline(&body)
            ));
            i += 1;
            continue;
        }

        if is_rule(line) {
            out.push("<hr>".to_string());
            i += 1;
            continue;
        }

        if strip_quote(line).is_some() {
            let start = i;
            let mut buf = Vec::new();
            while i < lines.len() {
                if let Some(inner) = strip_quote(lines[i]) {
                    buf.push(inner);
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(format!(
                "<blockquote>{}</blockquote>",
                render_blocks(&buf, offset + start)
            ));
            continue;
        }

        if is_table_start(lines, i) {
            let (html, next) = render_table(lines, i);
            out.push(html);
            i = next;
            continue;
        }

        if parse_list_item(line).is_some() {
            let (html, next) = render_list(lines, i, offset);
            out.push(html);
            i = next;
            continue;
        }

        let mut buf = Vec::new();
        while i < lines.len() && !is_blank(lines[i]) && (buf.is_empty() || !starts_block(lines, i)) {
            buf.push(lines[i].trim());
            i += 1;
        }
        if !buf.is_empty() {
            let joined = buf.join("\n");
            out.push(format!("<p>{}</p>", render_inline(&joined).replace('\n', "<br>")));
        } else {
            i += 1;
        }
    }

    out.join("\n")
}

pub fn render_markdown(source: &str) -> String {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "    ");
    if normalized.trim().is_empty() {
        return String::new();
    }
    let lines: Vec<&str> = normalized.split('\n').collect();
    render_blocks(&lines, 0)
}

pub fn plain_excerpt(source: &str, limit_chars: usize) -> String {
    for raw in source.replace("\r\n", "\n").split('\n') {
        let mut s = raw.trim().to_string();
        while s.starts_with('#') {
            s.remove(0);
        }
        s = s.trim_start_matches(['>', '-', '*', '+', ' ']).to_string();
        if let Some(rest) = s
            .strip_prefix("[ ] ")
            .or_else(|| s.strip_prefix("[x] "))
            .or_else(|| s.strip_prefix("[X] "))
        {
            s = rest.to_string();
        }
        s = s.replace(['*', '_', '~', '`'], "");
        let clean = s.trim();
        if !clean.is_empty() {
            let count = clean.chars().count();
            if count > limit_chars {
                let head: String = clean.chars().take(limit_chars).collect();
                return format!("{head}…");
            }
            return clean.to_string();
        }
    }
    String::new()
}

pub fn char_count(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

pub fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

pub fn rel_time(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }
    let now = js_sys::Date::now() as u64;
    let diff_sec = now.saturating_sub(ms) / 1000;
    if diff_sec < 60 {
        return "刚刚".to_string();
    }
    let min = diff_sec / 60;
    if min < 60 {
        return format!("{min} 分钟前");
    }
    let hour = min / 60;
    if hour < 24 {
        return format!("{hour} 小时前");
    }
    let day = hour / 24;
    if day < 30 {
        return format!("{day} 天前");
    }
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

pub fn abs_date(ms: u64) -> String {
    if ms == 0 {
        return "—".to_string();
    }
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes()
    )
}

#[cfg(test)]
mod tests {
    use super::render_inline;

    #[test]
    fn inline_multi_backtick_code_span() {
        let html = render_inline("`` `行内代码` `` → `行内代码`");
        assert_eq!(html, "<code>`行内代码`</code> → <code>行内代码</code>");
    }
}
