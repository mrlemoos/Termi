//! Markdown preview using terminal fonts and inert image/HTML placeholders.

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextFormat, text::LayoutJob};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

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
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut block, &mut blocks);
                style.code = true;
                block.literal = true;
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
}
