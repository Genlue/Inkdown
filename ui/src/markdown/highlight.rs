//! 零依赖离线代码高亮：支持主流语言的关键字、字符串、注释、数字着色。

pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn normalize_lang(lang: &str) -> &'static str {
    match lang.trim().to_ascii_lowercase().as_str() {
        "rs" | "rust" => "rust",
        "js" | "mjs" | "cjs" | "javascript" | "jsx" => "js",
        "ts" | "mts" | "cts" | "typescript" | "tsx" => "ts",
        "py" | "python" => "python",
        "json" | "jsonc" => "json",
        "css" | "scss" => "css",
        "html" | "htm" | "xml" | "svg" => "html",
        "sh" | "bash" | "zsh" | "pwsh" | "powershell" => "sh",
        "sql" => "sql",
        "toml" | "yaml" | "yml" | "ini" => "config",
        "md" | "markdown" => "markdown",
        _ => "",
    }
}

fn is_keyword(lang: &str, word: &str) -> bool {
    match lang {
        "rust" => matches!(
            word,
            "fn" | "let" | "mut" | "const" | "static" | "pub" | "struct" | "enum"
                | "impl" | "trait" | "for" | "in" | "while" | "loop" | "if" | "else"
                | "match" | "return" | "break" | "continue" | "use" | "mod" | "crate"
                | "super" | "self" | "Self" | "async" | "await" | "move" | "where"
                | "type" | "unsafe" | "true" | "false" | "Some" | "None" | "Ok" | "Err"
        ),
        "js" | "ts" => matches!(
            word,
            "const" | "let" | "var" | "function" | "return" | "if" | "else" | "for"
                | "while" | "do" | "switch" | "case" | "default" | "break" | "continue"
                | "import" | "export" | "from" | "as" | "class" | "extends" | "new"
                | "this" | "super" | "try" | "catch" | "finally" | "throw" | "async"
                | "await" | "typeof" | "instanceof" | "type" | "interface" | "enum"
                | "true" | "false" | "null" | "undefined"
        ),
        "python" => matches!(
            word,
            "def" | "class" | "return" | "if" | "elif" | "else" | "for" | "while"
                | "in" | "not" | "and" | "or" | "import" | "from" | "as" | "with"
                | "try" | "except" | "finally" | "raise" | "pass" | "break"
                | "continue" | "lambda" | "yield" | "async" | "await" | "True"
                | "False" | "None"
        ),
        "sql" => matches!(
            word.to_ascii_uppercase().as_str(),
            "SELECT" | "FROM" | "WHERE" | "INSERT" | "INTO" | "VALUES" | "UPDATE"
                | "SET" | "DELETE" | "CREATE" | "TABLE" | "JOIN" | "LEFT" | "RIGHT"
                | "INNER" | "ON" | "GROUP" | "BY" | "ORDER" | "LIMIT" | "AS" | "AND"
                | "OR" | "NOT" | "NULL" | "TRUE" | "FALSE"
        ),
        "sh" => matches!(
            word,
            "if" | "then" | "else" | "fi" | "for" | "in" | "do" | "done" | "case"
                | "esac" | "while" | "function" | "export" | "local" | "return"
                | "echo" | "cd" | "exit" | "true" | "false"
        ),
        "json" | "config" => matches!(word, "true" | "false" | "null"),
        _ => false,
    }
}

pub fn highlight_code(code: &str, raw_lang: &str) -> String {
    let lang = normalize_lang(raw_lang);
    if lang.is_empty() {
        return escape_html(code);
    }

    let chars: Vec<char> = code.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(code.len() + 64);
    let mut i = 0;

    while i < n {
        let ch = chars[i];

        // 行注释 // 或 #
        if (ch == '/' && i + 1 < n && chars[i + 1] == '/' && lang != "python" && lang != "sh")
            || ((ch == '#' && matches!(lang, "python" | "sh" | "config"))
                || (ch == '-' && i + 1 < n && chars[i + 1] == '-' && lang == "sql"))
        {
            let start = i;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            let seg: String = chars[start..i].iter().collect();
            out.push_str("<span class=\"hl-com\">");
            out.push_str(&escape_html(&seg));
            out.push_str("</span>");
            continue;
        }

        // 块注释 /* ... */
        if ch == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            let seg: String = chars[start..i].iter().collect();
            out.push_str("<span class=\"hl-com\">");
            out.push_str(&escape_html(&seg));
            out.push_str("</span>");
            continue;
        }

        // 字符串 "..." / '...' / `...`
        if ch == '"' || ch == '\'' || (ch == '`' && matches!(lang, "js" | "ts")) {
            let quote = ch;
            let start = i;
            i += 1;
            while i < n {
                if chars[i] == '\\' && i + 1 < n {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                if chars[i] == '\n' && quote != '`' {
                    break;
                }
                i += 1;
            }
            let seg: String = chars[start..i].iter().collect();
            out.push_str("<span class=\"hl-str\">");
            out.push_str(&escape_html(&seg));
            out.push_str("</span>");
            continue;
        }

        // 数字字面量
        if ch.is_ascii_digit()
            && (i == 0 || !(chars[i - 1].is_ascii_alphanumeric() || chars[i - 1] == '_'))
        {
            let start = i;
            while i < n
                && (chars[i].is_ascii_alphanumeric()
                    || chars[i] == '.'
                    || chars[i] == '_')
            {
                i += 1;
            }
            let seg: String = chars[start..i].iter().collect();
            out.push_str("<span class=\"hl-num\">");
            out.push_str(&escape_html(&seg));
            out.push_str("</span>");
            continue;
        }

        // 标识符 / 关键字
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if is_keyword(lang, &word) {
                out.push_str("<span class=\"hl-kw\">");
                out.push_str(&escape_html(&word));
                out.push_str("</span>");
            } else {
                out.push_str(&escape_html(&word));
            }
            continue;
        }

        out.push_str(&escape_html(&ch.to_string()));
        i += 1;
    }

    out
}
