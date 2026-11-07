//! Markdown preview using terminal fonts and inert image/HTML placeholders.
//! Fenced code is syntax-colored with syntect's built-in grammars.

use std::sync::OnceLock;

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextFormat, text::LayoutJob};
use egui_extras::syntax_highlighting::{CodeTheme, SyntectSettings};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Default)]
struct Span {
    text: String,
    bold: bool,
    emphasis: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Default)]
struct Block {
    spans: Vec<Span>,
    literal: bool,
    code: bool,
    lang: String,
}

fn flush(block: &mut Block, blocks: &mut Vec<Block>) {
    if !block.spans.is_empty() {
        blocks.push(std::mem::take(block));
    }
}

fn append(block: &mut Block, style: &Span, text: impl Into<String>) {
    block.spans.push(Span { text: text.into(), ..style.clone() });
}

fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Span::default();
    let mut styles = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut image: Option<String> = None;
    let mut html = false;
    let mut table: Vec<Vec<Block>> = Vec::new();
    let mut row = Vec::new();
    for event in Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH) {
        if let Some(alt) = &mut image {
            match event {
                Event::End(TagEnd::Image) => {
                    let alt = image.take().unwrap();
                    append(&mut block, &style, format!("[Image: {}]", if alt.trim().is_empty() { "no alt text" } else { alt.trim() }));
                }
                Event::Text(s) | Event::Code(s) => alt.push_str(&s),
                Event::SoftBreak | Event::HardBreak => alt.push(' '),
                _ => {}
            }
            continue;
        }
        if html {
            if event == Event::End(TagEnd::HtmlBlock) { html = false; }
            continue;
        }
        match event {
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => flush(&mut block, &mut blocks),
            Event::Start(Tag::Heading { .. }) => { flush(&mut block, &mut blocks); styles.push(style.clone()); style.bold = true; }
            Event::End(TagEnd::Heading(_)) => { flush(&mut block, &mut blocks); style = styles.pop().unwrap_or_default(); }
            Event::Start(Tag::Emphasis) => { styles.push(style.clone()); style.emphasis = true; }
            Event::Start(Tag::Strong) => { styles.push(style.clone()); style.bold = true; }
            Event::Start(Tag::Strikethrough) => { styles.push(style.clone()); style.strike = true; }
            Event::Start(Tag::Link { dest_url, .. }) => { styles.push(style.clone()); style.link = Some(dest_url.into_string()); }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link) => style = styles.pop().unwrap_or_default(),
            Event::Start(Tag::Image { .. }) => image = Some(String::new()),
            Event::Start(Tag::HtmlBlock) => {
                flush(&mut block, &mut blocks);
                append(&mut block, &style, "[HTML: not rendered]");
                flush(&mut block, &mut blocks);
                html = true;
            }
            Event::Html(_) | Event::InlineHtml(_) => append(&mut block, &style, "[HTML: not rendered]"),
            Event::Start(Tag::CodeBlock(kind)) => {
                flush(&mut block, &mut blocks);
                style.code = true;
                block.literal = true;
                block.code = true;
                block.lang = fence_lang(&kind);
            }
            Event::End(TagEnd::CodeBlock) => { flush(&mut block, &mut blocks); style.code = false; }
            Event::Start(Tag::List(start)) => { flush(&mut block, &mut blocks); lists.push(start); }
            Event::End(TagEnd::List(_)) => { flush(&mut block, &mut blocks); lists.pop(); }
            Event::Start(Tag::Item) => {
                flush(&mut block, &mut blocks);
                let indent = "  ".repeat(lists.len().saturating_sub(1));
                let marker = match lists.last_mut() {
                    Some(Some(n)) => { let marker = format!("{n}."); *n += 1; marker }
                    _ => "•".into(),
                };
                append(&mut block, &style, format!("{indent}{marker} "));
            }
            Event::End(TagEnd::Item) => flush(&mut block, &mut blocks),
            Event::Start(Tag::BlockQuote(_)) => { flush(&mut block, &mut blocks); append(&mut block, &style, "│ "); }
            Event::End(TagEnd::BlockQuote(_)) => flush(&mut block, &mut blocks),
            Event::Start(Tag::Table(_)) => { flush(&mut block, &mut blocks); table.clear(); }
            Event::Start(Tag::TableHead) => style.bold = true,
            Event::End(TagEnd::TableCell) => row.push(std::mem::take(&mut block)),
            Event::End(TagEnd::TableHead) => { table.push(std::mem::take(&mut row)); style.bold = false; }
            Event::End(TagEnd::TableRow) => table.push(std::mem::take(&mut row)),
            Event::End(TagEnd::Table) => {
                let cols = table.iter().map(Vec::len).max().unwrap_or(0);
                let widths: Vec<usize> = (0..cols).map(|col| table.iter().filter_map(|r| r.get(col))
                    .map(|cell| cell.spans.iter().map(|s| s.text.chars().count()).sum()).max().unwrap_or(0)).collect();
                for cells in table.drain(..) {
                    let mut line = Block { literal: true, ..Block::default() };
                    append(&mut line, &Span::default(), "│ ");
                    for (col, cell) in cells.into_iter().enumerate() {
                        let len: usize = cell.spans.iter().map(|s| s.text.chars().count()).sum();
                        line.spans.extend(cell.spans);
                        append(&mut line, &Span::default(), format!("{} │ ", " ".repeat(widths[col] - len)));
                    }
                    blocks.push(line);
                }
            }
            Event::Text(s) => append(&mut block, &style, s.into_string()),
            Event::Code(s) => append(&mut block, &Span { code: true, ..style.clone() }, s.into_string()),
            Event::SoftBreak => append(&mut block, &style, " "),
            Event::HardBreak => append(&mut block, &style, "\n"),
            Event::TaskListMarker(done) => append(&mut block, &style, if done { "[x] " } else { "[ ] " }),
            Event::Rule => { flush(&mut block, &mut blocks); append(&mut block, &style, "────────────────"); flush(&mut block, &mut blocks); }
            _ => {}
        }
    }
    flush(&mut block, &mut blocks);
    blocks
}

/// Returns a clicked destination; the editor decides how to navigate safely.
pub fn show(ui: &mut egui::Ui, text: &str, font: &FontId) -> Option<String> {
    let mut clicked = None;
    for block in parse(text) {
        if block.code {
            let code: String = block.spans.iter().map(|s| s.text.as_str()).collect();
            let mut job = code_layout(ui.ctx(), ui.style(), &code, &block.lang, font);
            job.wrap.max_width = f32::INFINITY;
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            ui.add(egui::Label::new(galley).selectable(true));
            ui.add_space(font.size * 0.5);
            continue;
        }
        let mut job = LayoutJob::default();
        job.wrap.max_width = if block.literal { f32::INFINITY } else { ui.available_width() };
        let mut links = Vec::new();
        let mut chars = 0;
        for span in block.spans {
            let color = if span.link.is_some() { Color32::from_rgb(0x26, 0x8b, 0xd2) } else { crate::term::foreground() };
            let format = TextFormat {
                font_id: if span.bold { FontId::new(font.size, FontFamily::Name("bold".into())) } else { font.clone() },
                color,
                underline: if span.emphasis || span.link.is_some() { Stroke::new(1.0, color) } else { Stroke::NONE },
                strikethrough: if span.strike { Stroke::new(1.0, color) } else { Stroke::NONE },
                background: if span.code { Color32::from_white_alpha(12) } else { Color32::TRANSPARENT },
                ..TextFormat::default()
            };
            let end = chars + span.text.chars().count();
            if let Some(link) = span.link { links.push((chars..end, link)); }
            chars = end;
            job.append(&span.text, 0.0, format);
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let response = ui.add(egui::Label::new(galley.clone()).sense(egui::Sense::click()).selectable(true));
        if let Some(pos) = response.hover_pos() {
            let index = galley.cursor_from_pos(pos - response.rect.min).index.0;
            if let Some((_, link)) = links.iter().find(|(range, _)| range.contains(&index)) {
                response.clone().on_hover_text(link).on_hover_cursor(egui::CursorIcon::PointingHand);
                if response.clicked() { clicked = Some(link.clone()); }
            }
        }
        ui.add_space(font.size * 0.5);
    }
    clicked
}

fn fence_lang(kind: &CodeBlockKind) -> String {
    match kind {
        // Keep the full tag so multi-word grammar names still resolve. Drop a `{...}` attribute tail.
        CodeBlockKind::Fenced(info) => info.trim().split('{').next().unwrap_or("").trim().trim_end_matches(',').to_string(),
        CodeBlockKind::Indented => String::new(),
    }
}

pub(crate) fn syntect_settings() -> &'static SyntectSettings {
    static SETTINGS: OnceLock<SyntectSettings> = OnceLock::new();
    // bat's grammar pack (two-face), not syntect's smaller default set.
    SETTINGS.get_or_init(|| SyntectSettings {
        ps: two_face::syntax::extra_newlines(),
        ts: syntect::highlighting::ThemeSet::load_defaults(),
    })
}

/// Fence token -> syntect syntax name. Name match is case-insensitive; extension match is too.
pub(crate) fn resolve(token: &str) -> Option<&'static str> {
    let token = token.split([',', '{']).next()?.trim();
    if token.is_empty() {
        return None;
    }
    let ps = &syntect_settings().ps;
    // Exact name wins over an extension collision (`ASP` vs `HTML (ASP)`).
    if let Some(syntax) = ps.find_syntax_by_name(token) {
        return Some(syntax.name.as_str());
    }
    if let Some(syntax) = ps.find_syntax_by_token(token) {
        return Some(syntax.name.as_str());
    }
    let name = alias(token)?;
    ps.find_syntax_by_name(name).map(|syntax| syntax.name.as_str())
}

/// Tags that are neither a syntect name nor a file extension.
fn alias(token: &str) -> Option<&'static str> {
    Some(match token.to_ascii_lowercase().as_str() {
        "golang" => "Go",
        "csharp" | "c#" => "C#",
        "fsharp" | "f#" => "F#",
        "cplusplus" | "c++" => "C++",
        "objc" | "objectivec" | "objective-c" => "Objective-C",
        "objcpp" | "objectivec++" | "objective-c++" => "Objective-C++",
        "shell" | "zsh" | "console" | "shellsession" => "Bourne Again Shell (bash)",
        "docker" | "dockerfile" => "Dockerfile",
        "make" | "mk" | "makefile" => "Makefile",
        "protobuf" | "proto" => "Protocol Buffer",
        "gql" | "graphql" => "GraphQL",
        "tf" | "terraform" | "hcl" => "Terraform",
        "yml" => "YAML",
        "md" => "Markdown",
        "py3" | "python3" => "Python",
        "node" | "jsx" => "JavaScript",
        "text" | "plain" | "plaintext" => "Plain Text",
        "rs" => "Rust",
        "gdscript" => "GDScript (Godot Engine)",
        "fortran" => "Fortran (Modern)",
        _ => return None,
    })
}

fn code_layout(ctx: &egui::Context, style: &egui::Style, code: &str, lang: &str, font: &FontId) -> LayoutJob {
    let theme = (if crate::term::theme().light { CodeTheme::light(font.size) } else { CodeTheme::dark(font.size) }).with_font_id(font.clone());
    let mut job = match resolve(lang) {
        Some(name) => egui_extras::syntax_highlighting::highlight_with(ctx, style, &theme, code, name, syntect_settings()),
        None => LayoutJob::simple(code.to_owned(), font.clone(), crate::term::foreground(), f32::INFINITY),
    };
    // No italic face in the terminal font. egui_extras underlines italic tokens.
    for section in &mut job.sections {
        section.format.italics = false;
        section.format.underline = Stroke::NONE;
        section.format.font_id = font.clone();
        section.format.background = Color32::from_white_alpha(12);
    }
    job.wrap.max_width = f32::INFINITY;
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_syntax_and_inert_placeholders() {
        let blocks = parse("# Heading\n\n**bold** *em* ~~gone~~ [link](next.md)\n\n- [x] task\n\n![cat](https://example.com/cat.png) ![](empty.png)\n\n<div>secret</div>\n\n```rs\nlet x = 1;\n```\n\n| A | Long |\n|---|---|\n| B | C |\n");
        let output: String = blocks.iter().flat_map(|b| &b.spans).map(|s| s.text.as_str()).collect();
        assert!(output.contains("Heading"));
        assert!(blocks[0].spans[0].bold);
        assert!(blocks.iter().flat_map(|b| &b.spans).any(|s| s.emphasis && s.text == "em"));
        assert!(blocks.iter().flat_map(|b| &b.spans).any(|s| s.link.as_deref() == Some("next.md")));
        assert!(output.contains("[x] task"));
        assert!(output.contains("[Image: cat] [Image: no alt text]"));
        assert!(output.contains("[HTML: not rendered]"));
        assert!(!output.contains("secret"));
        assert!(output.contains("let x = 1;"));
        assert!(output.contains("│ B │ C    │"));
    }

    #[test]
    fn nested_styles_restore_outer_formatting() {
        let blocks = parse("# Heading **bold** tail\n\n**outer *inner* outer**\n\n1. first\n2. second\n   - nested\n");
        assert!(blocks[0].spans.iter().all(|s| s.bold));
        assert!(blocks[1].spans.iter().all(|s| s.bold));
        assert!(!blocks[1].spans.last().unwrap().emphasis);
        let text: String = blocks.iter().flat_map(|b| &b.spans).map(|s| s.text.as_str()).collect();
        assert!(text.contains("1. first2. second  • nested"));
    }

    #[test]
    fn preview_renders_terminal_layout() {
        let _theme = crate::term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = "# Markdown in Termi\n\nRead **bold**, *emphasis*, ~~removed~~ and `inline code`. [Open guide](guide.md).\n\nA longer paragraph wraps inside the preview pane while keeping every word readable and aligned with the terminal font. Source stays editable behind the toggle.\n\n## Checklist\n\n- [x] Render Markdown\n- [ ] Edit source\n  - Keep unsaved changes\n\n1. Preview\n2. Source\n\n> Keep terminal styling.\n\n```typescript\nconst mode = 'preview';\nconsole.log(mode);\n```\n\n| View | Editable |\n| --- | --- |\n| Preview | No |\n| Source | Yes |\n\n![Diagram](diagram.png) ![](empty.png)\n\n<div>hidden</div>\n";
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(680.0, 720.0)).build_ui_state(move |ui, ready: &mut bool| {
            if *ready {
                ui.painter().rect_filled(ui.max_rect(), 0.0, crate::term::background());
                show(ui, text, &FontId::monospace(14.0));
            }
        }, false);
        crate::apply(&h.ctx, &crate::settings::Settings::default());
        *h.state_mut() = true;
        h.run();
        h.render().unwrap().save("/tmp/termi-markdown-layout.png").unwrap();
    }

    #[test]
    fn code_fence_keeps_language_token() {
        let blocks = parse("```rust\nfn main() {}\n```\n\n    indented\n\n```Bourne Again Shell (bash)\necho hi\n```\n");
        assert!(blocks.iter().any(|b| b.code && b.lang == "rust" && b.spans.iter().any(|s| s.text.contains("fn main"))));
        assert!(blocks.iter().any(|b| b.code && b.lang.is_empty() && b.spans.iter().any(|s| s.text.contains("indented"))));
        assert!(blocks.iter().any(|b| b.lang == "Bourne Again Shell (bash)"));
    }

    #[test]
    fn every_builtin_grammar_resolves() {
        let ps = &syntect_settings().ps;
        for syntax in ps.syntaxes() {
            assert_eq!(resolve(&syntax.name).map(str::to_owned), Some(syntax.name.clone()), "{}", syntax.name);
            for ext in &syntax.file_extensions {
                assert!(resolve(ext).is_some(), "{ext} ({})", syntax.name);
            }
        }
    }

    #[test]
    fn snippets_cover_every_visible_grammar() {
        let fenced: std::collections::HashSet<_> = parse(include_str!("../docs/snippets.md")).into_iter().filter(|b| b.code).map(|b| b.lang).collect();
        for syntax in syntect_settings().ps.syntaxes() {
            if !syntax.hidden {
                assert!(fenced.contains(&syntax.name), "missing {}", syntax.name);
            }
        }
    }

    #[test]
    fn common_fence_tags_resolve() {
        for tag in [
            "rust", "rs", "python", "py", "python3", "javascript", "js", "jsx", "node",
            "typescript", "ts", "tsx", "go", "golang", "java", "c", "cpp", "c++", "cplusplus",
            "cs", "csharp", "c#", "fs", "f#", "fsharp", "ruby", "rb", "php", "swift",
            "kotlin", "kt", "scala", "haskell", "hs", "lua", "perl", "pl", "r", "sql",
            "html", "css", "scss", "sass", "less", "json", "yaml", "yml", "toml", "xml",
            "sh", "bash", "zsh", "shell", "fish", "dockerfile", "docker", "makefile", "make",
            "cmake", "dart", "elixir", "ex", "erlang", "erl", "clojure", "clj", "lisp",
            "ocaml", "ml", "nim", "zig", "crystal", "cr", "groovy", "ini", "diff",
            "markdown", "md", "tex", "latex", "matlab", "julia", "jl", "proto", "protobuf",
            "graphql", "gql", "vue", "svelte", "nix", "terraform", "tf", "hcl", "nginx",
            "objc", "objective-c", "mm", "glsl", "wgsl", "solidity", "sol", "elm",
            "purescript", "purs", "fortran", "f90", "pascal", "pas", "ada", "racket", "rkt",
            "lean", "idris", "idr", "odin", "vhdl", "verilog", "llvm", "ll", "jq", "qml",
            "typst", "typ", "cabal", "awk", "env", "csv", "tsv", "org", "rst", "vim",
            "puppet", "pp", "rego", "vyper", "vy", "asm", "nasm", "jsonnet", "gdscript", "gd",
            "text", "plain", "plaintext", "txt", "console",
        ] {
            assert!(resolve(tag).is_some(), "{tag}");
        }
    }

    #[test]
    fn highlighted_fence_uses_more_than_one_color() {
        let _theme = crate::term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let samples = [
            ("rust", "fn main() {\n    let x = \"hi\";\n}\n"),
            ("typescript", "const mode: string = \"preview\";\n"),
            ("python", "def main():\n    return \"hi\"\n"),
            ("toml", "[package]\nname = \"termi\"\n"),
            ("sql", "select name from tabs where id = 1;\n"),
        ];
        let mut h = egui_kittest::Harness::builder().build_ui_state(move |ui, jobs: &mut Vec<LayoutJob>| {
            *jobs = samples.iter().map(|(lang, code)| code_layout(ui.ctx(), ui.style(), code, lang, &FontId::monospace(14.0))).collect();
        }, Vec::new());
        crate::apply(&h.ctx, &crate::settings::Settings::default());
        h.run();
        for (job, (lang, _)) in h.state().iter().zip(samples) {
            let colors: std::collections::HashSet<_> = job.sections.iter().map(|s| s.format.color).collect();
            assert!(colors.len() > 1, "{lang}: {colors:?}");
            assert!(job.sections.iter().all(|s| !s.format.italics && s.format.underline == Stroke::NONE));
        }
        let style = h.ctx.global_style();
        let plain = code_layout(&h.ctx, &style, "fn main() {}\n", "not-a-language", &FontId::monospace(14.0));
        assert!(plain.sections.iter().all(|s| s.format.color == crate::term::foreground()));
    }
}
