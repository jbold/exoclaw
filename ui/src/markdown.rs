use pulldown_cmark::{Event, Parser, html};

pub fn render(input: &str) -> String {
    // Convert raw HTML events to escaped text to prevent XSS.
    // pulldown-cmark passes through raw HTML by default; converting
    // Html/InlineHtml to Text causes push_html to escape them.
    let parser = Parser::new(input).map(|event| match event {
        Event::Html(html) | Event::InlineHtml(html) => Event::Text(html),
        other => other,
    });
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}
