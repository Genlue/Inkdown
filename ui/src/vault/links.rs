//! 全仓库双向链接、标签与大纲标题索引（纯 Rust，UTF-8 安全）。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikilinkRef {
    pub target: String,
    pub alias: Option<String>,
    pub heading: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadingInfo {
    pub level: u8,
    pub text: String,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct IndexedNote {
    pub title: String,
    pub links: Vec<WikilinkRef>,
    pub headings: Vec<HeadingInfo>,
    pub content: String,
}

#[derive(Clone, Debug, Default)]
pub struct LinkIndex {
    pub by_path: BTreeMap<String, IndexedNote>,
    /// target_path -> set of source_paths
    pub backlinks: HashMap<String, BTreeSet<String>>,
    /// unresolved target -> set of source_paths
    pub unresolved: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BacklinkItem {
    pub path: String,
    pub title: String,
    pub snippets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlinkItem {
    pub target: String,
    pub path: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DanglingLink {
    pub target: String,
    pub count: usize,
}

pub struct IndexInput {
    pub path: String,
    pub title: String,
    pub content: String,
}

/* ---------------- 预处理（擦除围栏代码 / 行内代码 / HTML 注释） ---------------- */

fn to_lines(source: &str) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }
    source
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(str::to_string)
        .collect()
}

fn parse_fence_line(line: &str) -> Option<(char, usize, &str)> {
    let trimmed = line.trim_start();
    let first = trimmed.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let width = trimmed.chars().take_while(|&c| c == first).count();
    if width < 3 {
        return None;
    }
    let rest = &trimmed[width..];
    Some((first, width, rest))
}

fn fence_flags(lines: &[String]) -> Vec<bool> {
    let mut flags = vec![false; lines.len()];
    let mut mark: Option<(char, usize)> = None;

    for (i, line) in lines.iter().enumerate() {
        let fence = parse_fence_line(line);
        match mark {
            None => {
                if let Some((ch, width, rest)) = fence {
                    if !(ch == '`' && rest.contains('`')) {
                        mark = Some((ch, width));
                        flags[i] = true;
                    }
                }
            }
            Some((open_ch, open_w)) => {
                flags[i] = true;
                if let Some((ch, width, rest)) = fence {
                    if ch == open_ch && width >= open_w && rest.trim().is_empty() {
                        mark = None;
                    }
                }
            }
        }
    }
    flags
}

fn mask_inline_code(line: &str) -> String {
    if !line.contains('`') {
        return line.to_string();
    }
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let mut ticks = 0;
            while i + ticks < bytes.len() && bytes[i + ticks] == b'`' {
                ticks += 1;
            }
            // 查找匹配的闭合反引号串
            let mut j = i + ticks;
            let mut found = None;
            while j < bytes.len() {
                if bytes[j] == b'`' {
                    let mut close_ticks = 0;
                    while j + close_ticks < bytes.len() && bytes[j + close_ticks] == b'`' {
                        close_ticks += 1;
                    }
                    if close_ticks == ticks {
                        found = Some(j + close_ticks);
                        break;
                    }
                    j += close_ticks;
                } else {
                    j += 1;
                }
            }
            if let Some(end) = found {
                let char_len = line[i..end].chars().count();
                for _ in 0..char_len {
                    out.push(' ');
                }
                i = end;
                continue;
            } else {
                for _ in 0..ticks {
                    out.push('`');
                }
                i += ticks;
                continue;
            }
        }
        let ch = line[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn mask_comments(text: &str) -> String {
    if !text.contains("<!--") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some(rel_start) = text[i..].find("<!--") {
            let start = i + rel_start;
            out.push_str(&text[i..start]);
            let end = text[start + 4..]
                .find("-->")
                .map(|r| start + 4 + r + 3)
                .unwrap_or(text.len());
            for ch in text[start..end].chars() {
                out.push(if ch == '\n' { '\n' } else { ' ' });
            }
            i = end;
        } else {
            out.push_str(&text[i..]);
            break;
        }
    }
    out
}

fn scan(source: &str) -> (Vec<String>, Vec<bool>, String) {
    let lines = to_lines(source);
    if lines.is_empty() {
        return (lines, Vec::new(), String::new());
    }
    let fenced = fence_flags(&lines);
    let masked_lines: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if fenced[i] {
                " ".repeat(line.chars().count())
            } else {
                mask_inline_code(line)
            }
        })
        .collect();
    let masked = mask_comments(&masked_lines.join("\n"));
    (lines, fenced, masked)
}

/* ---------------- 提取 ---------------- */

pub fn extract_wikilinks(markdown: &str) -> Vec<WikilinkRef> {
    let (_, _, masked) = scan(markdown);
    collect_links(&masked)
}

fn collect_links(text: &str) -> Vec<WikilinkRef> {
    let mut out = Vec::new();
    if !text.contains("[[") {
        return out;
    }
    let mut i = 0;
    while let Some(rel) = text[i..].find("[[") {
        let start = i + rel;
        let is_embed = start > 0 && text.as_bytes()[start - 1] == b'!';
        let content_start = start + 2;
        let Some(rel_end) = text[content_start..].find("]]") else {
            break;
        };
        let end = content_start + rel_end;
        let inner_raw = &text[content_start..end];
        i = end + 2;

        if is_embed || inner_raw.contains('\n') || inner_raw.contains('[') {
            continue;
        }

        let mut inner = inner_raw;
        let mut alias = None;
        let mut heading = None;

        if let Some(bar) = inner.find('|') {
            let a = inner[bar + 1..].trim();
            if !a.is_empty() {
                alias = Some(a.to_string());
            }
            inner = &inner[..bar];
        }
        if let Some(hash) = inner.find('#') {
            let h = inner[hash + 1..].trim();
            if !h.is_empty() {
                heading = Some(h.to_string());
            }
            inner = &inner[..hash];
        }
        let target = inner.trim();
        if target.is_empty() {
            continue;
        }
        out.push(WikilinkRef {
            target: target.to_string(),
            alias,
            heading,
        });
    }
    out
}

fn plain_heading(text: &str) -> String {
    let mut s = text.to_string();
    // 去掉行内反引号与格式符，保留纯文本大纲标题
    s = s.replace(['`', '*', '_', '~'], "");
    // 简化 [[target|alias]] -> alias/target
    while let Some(start) = s.find("[[") {
        let Some(rel_end) = s[start + 2..].find("]]") else {
            break;
        };
        let end = start + 2 + rel_end;
        let inner = &s[start + 2..end];
        let display = inner
            .split('|')
            .nth(1)
            .unwrap_or_else(|| inner.split('#').next().unwrap_or(inner))
            .trim()
            .to_string();
        let prefix = if start > 0 && s.as_bytes()[start - 1] == b'!' {
            start - 1
        } else {
            start
        };
        s.replace_range(prefix..end + 2, &display);
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn extract_headings(markdown: &str) -> Vec<HeadingInfo> {
    let (lines, fenced, _) = scan(markdown);
    collect_headings(&lines, &fenced)
}

fn collect_headings(lines: &[String], fenced: &[bool]) -> Vec<HeadingInfo> {
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if fenced.get(i).copied().unwrap_or(false) {
            continue;
        }
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() > 3 {
            continue;
        }
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if !(1..=6).contains(&level) {
            continue;
        }
        let rest = &trimmed[level..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let body = rest.trim();
        // 去掉末尾可选闭合 `#`
        let body = body
            .trim_end_matches('#')
            .trim_end_matches(char::is_whitespace);
        let clean = plain_heading(body);
        if !clean.is_empty() {
            out.push(HeadingInfo {
                level: level as u8,
                text: clean,
                line: i,
            });
        }
    }
    out
}

/* ---------------- 路径匹配与索引 ---------------- */

fn normalize_path(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.trim_start_matches('/').to_string()
}

fn note_name(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.strip_suffix(".md")
        .or_else(|| base.strip_suffix(".MD"))
        .or_else(|| base.strip_suffix(".markdown"))
        .unwrap_or(base)
        .to_string()
}

fn normalize_target(target: &str) -> String {
    let mut s = target.trim().replace('\\', "/");
    while s.starts_with('/') {
        s.remove(0);
    }
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    s = s.trim_end_matches('/').to_string();
    if let Some(stem) = s
        .strip_suffix(".md")
        .or_else(|| s.strip_suffix(".MD"))
        .or_else(|| s.strip_suffix(".markdown"))
    {
        s = stem.to_string();
    }
    s.trim().to_string()
}

struct Lookup {
    by_name: HashMap<String, Vec<String>>,
    by_exact: HashMap<String, String>,
}

fn build_lookup(by_path: &BTreeMap<String, IndexedNote>) -> Lookup {
    let mut by_name: HashMap<String, Vec<String>> = HashMap::new();
    let mut by_exact: HashMap<String, String> = HashMap::new();

    for path in by_path.keys() {
        let lower = path.to_lowercase();
        by_exact.insert(lower.clone(), path.clone());
        if let Some(stem) = lower.strip_suffix(".md") {
            by_exact.insert(stem.to_string(), path.clone());
        }
        let name = note_name(path).to_lowercase();
        by_name.entry(name).or_default().push(path.clone());
    }

    for bucket in by_name.values_mut() {
        bucket.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    }
    Lookup { by_name, by_exact }
}

fn resolve_target(target: &str, lookup: &Lookup) -> Vec<String> {
    let norm = normalize_target(target);
    if norm.is_empty() {
        return Vec::new();
    }
    let lower = norm.to_lowercase();
    if lower.contains('/') {
        if let Some(exact) = lookup.by_exact.get(&lower) {
            return vec![exact.clone()];
        }
    }
    let name_key = lower.rsplit('/').next().unwrap_or(&lower);
    lookup.by_name.get(name_key).cloned().unwrap_or_default()
}

pub fn build_index(entries: &[IndexInput]) -> LinkIndex {
    let mut by_path = BTreeMap::new();
    let mut backlinks: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut unresolved: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for entry in entries {
        let path = normalize_path(&entry.path);
        if path.is_empty() {
            continue;
        }
        let (lines, fenced, masked) = scan(&entry.content);
        let title = if entry.title.trim().is_empty() {
            note_name(&path)
        } else {
            entry.title.trim().to_string()
        };
        by_path.insert(
            path,
            IndexedNote {
                title,
                links: collect_links(&masked),
                headings: collect_headings(&lines, &fenced),
                content: entry.content.clone(),
            },
        );
    }

    let lookup = build_lookup(&by_path);

    for (source, info) in &by_path {
        for link in &info.links {
            let hits = resolve_target(&link.target, &lookup);
            if hits.is_empty() {
                unresolved
                    .entry(link.target.clone())
                    .or_default()
                    .insert(source.clone());
                continue;
            }
            for target in hits {
                if &target != source {
                    backlinks
                        .entry(target)
                        .or_default()
                        .insert(source.clone());
                }
            }
        }
    }

    LinkIndex {
        by_path,
        backlinks,
        unresolved,
    }
}

pub fn find_backlinks(index: &LinkIndex, path: &str) -> Vec<BacklinkItem> {
    let key = normalize_path(path);
    let Some(sources) = index.backlinks.get(&key) else {
        return Vec::new();
    };
    let lookup = build_lookup(&index.by_path);
    let mut out = Vec::new();

    for source in sources {
        let Some(info) = index.by_path.get(source) else {
            continue;
        };
        let mut snippets = Vec::new();
        let mut seen = HashSet::new();
        for link in &info.links {
            if !resolve_target(&link.target, &lookup).contains(&key) {
                continue;
            }
            let text = snippet_around(&info.content, &link.target, 56);
            if !text.is_empty() && seen.insert(text.clone()) {
                snippets.push(text);
                if snippets.len() >= 3 {
                    break;
                }
            }
        }
        out.push(BacklinkItem {
            path: source.clone(),
            title: info.title.clone(),
            snippets,
        });
    }
    out
}

pub fn find_outlinks(index: &LinkIndex, path: &str) -> Vec<OutlinkItem> {
    let key = normalize_path(path);
    let Some(info) = index.by_path.get(&key) else {
        return Vec::new();
    };
    let lookup = build_lookup(&index.by_path);
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for link in &info.links {
        let hits = resolve_target(&link.target, &lookup);
        if hits.is_empty() {
            let dedup = format!("{}\0", link.target);
            if seen.insert(dedup) {
                out.push(OutlinkItem {
                    target: link.target.clone(),
                    path: None,
                    title: link.alias.clone().unwrap_or_else(|| link.target.clone()),
                });
            }
            continue;
        }
        for target in hits {
            let dedup = format!("{}\0{}", link.target, target);
            if seen.insert(dedup) {
                let title = index
                    .by_path
                    .get(&target)
                    .map(|n| n.title.clone())
                    .or_else(|| link.alias.clone())
                    .unwrap_or_else(|| link.target.clone());
                out.push(OutlinkItem {
                    target: link.target.clone(),
                    path: Some(target),
                    title,
                });
            }
        }
    }
    out
}

pub fn resolve_wikilink_path(index: &LinkIndex, target: &str) -> Option<String> {
    let lookup = build_lookup(&index.by_path);
    resolve_target(target, &lookup).into_iter().next()
}

pub fn all_dangling_sorted(index: &LinkIndex) -> Vec<DanglingLink> {
    let mut items: Vec<DanglingLink> = index
        .unresolved
        .iter()
        .map(|(target, set)| DanglingLink {
            target: target.clone(),
            count: set.len(),
        })
        .collect();
    items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));
    items
}

/// 按字符边界截取命中词上下文，绝不在多字节 UTF-8 中间切片。
pub fn snippet_around(content: &str, needle: &str, radius_chars: usize) -> String {
    let word = needle.trim();
    if content.is_empty() || word.is_empty() {
        return String::new();
    }
    let byte_at = if let Some(pos) = content.find(word) {
        pos
    } else {
        // ASCII 大小写不敏感兜底，UTF-8 安全逐字符比对
        let word_lc = word.to_lowercase();
        let word_chars = word_lc.chars().count();
        let indices: Vec<(usize, char)> = content.char_indices().collect();
        let mut found = None;
        for start_ci in 0..indices.len() {
            if start_ci + word_chars > indices.len() {
                break;
            }
            let b_start = indices[start_ci].0;
            let b_end = if start_ci + word_chars < indices.len() {
                indices[start_ci + word_chars].0
            } else {
                content.len()
            };
            if content[b_start..b_end].to_lowercase() == word_lc {
                found = Some(b_start);
                break;
            }
        }
        match found {
            Some(pos) => pos,
            None => return String::new(),
        }
    };

    let char_pos = content[..byte_at].chars().count();
    let word_len_chars = word.chars().count();
    let total_chars = content.chars().count();
    let from = char_pos.saturating_sub(radius_chars);
    let to = (char_pos + word_len_chars + radius_chars).min(total_chars);

    let slice: String = content
        .chars()
        .skip(from)
        .take(to - from)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if slice.is_empty() {
        return String::new();
    }
    format!(
        "{}{}{}",
        if from > 0 { "…" } else { "" },
        slice,
        if to < total_chars { "…" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_wikilinks_and_ignores_code() {
        let md = "参见 [[笔记A|别名]] 和 `[[忽略我]]` 以及 ![[pic.png]]\n```\n[[代码块里的]]\n```\n";
        let links = extract_wikilinks(md);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "笔记A");
        assert_eq!(links[0].alias.as_deref(), Some("别名"));
    }

    #[test]
    fn utf8_snippet_does_not_panic() {
        let s = snippet_around("这是一段包含中文和[[欢迎使用 Inkdown]]的长文本内容。", "Inkdown", 6);
        assert!(s.contains("Inkdown"));
    }
}
