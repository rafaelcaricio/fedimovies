use std::cell::RefCell;

use comrak::{
    arena_tree::Node,
    format_commonmark, format_html,
    nodes::{Ast, AstNode, ListType, NodeValue},
    parse_document, Arena, ComrakExtensionOptions, ComrakOptions, ComrakParseOptions,
    ComrakRenderOptions,
};

#[derive(thiserror::Error, Debug)]
pub enum MarkdownError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

fn build_comrak_options() -> ComrakOptions {
    ComrakOptions {
        extension: ComrakExtensionOptions {
            autolink: true,
            ..Default::default()
        },
        parse: ComrakParseOptions::default(),
        render: ComrakRenderOptions {
            hardbreaks: true,
            escape: true,
            ..Default::default()
        },
    }
}

fn iter_nodes<'a, F>(node: &'a AstNode<'a>, func: &F) -> Result<(), MarkdownError>
where
    F: Fn(&'a AstNode<'a>) -> Result<(), MarkdownError>,
{
    func(node)?;
    for child in node.children() {
        iter_nodes(child, func)?;
    }
    Ok(())
}

fn node_to_markdown<'a>(
    node: &'a AstNode<'a>,
    options: &ComrakOptions,
) -> Result<String, MarkdownError> {
    let mut output = vec![];
    format_commonmark(node, options, &mut output)?;
    let markdown = String::from_utf8(output)?
        .trim_end_matches('\n')
        .to_string();
    Ok(markdown)
}

fn replace_node_value(node: &AstNode, value: NodeValue) -> () {
    let mut borrowed_node = node.data.borrow_mut();
    *borrowed_node = Ast::new(value, borrowed_node.sourcepos.start);
}

fn create_node<'a>(value: NodeValue) -> AstNode<'a> {
    // Position doesn't matter
    Node::new(RefCell::new(Ast::new(value, (0, 1).into())))
}

fn replace_with_markdown<'a>(
    node: &'a AstNode<'a>,
    options: &ComrakOptions,
) -> Result<(), MarkdownError> {
    // Replace node with text node containing markdown
    let markdown = node_to_markdown(node, options)?;
    for child in node.children() {
        child.detach();
    }
    let text = NodeValue::Text(markdown);
    replace_node_value(node, text);
    Ok(())
}

fn fix_microsyntaxes<'a>(node: &'a AstNode<'a>) -> Result<(), MarkdownError> {
    if let Some(prev) = node.previous_sibling() {
        if let NodeValue::Text(ref prev_text) = prev.data.borrow().value {
            // Remove autolink if mention or object link syntax is found
            if prev_text.ends_with('@') || prev_text.ends_with("[[") {
                let mut link_text = String::new();
                for child in node.children() {
                    child.detach();
                    let child_value = &child.data.borrow().value;
                    if let NodeValue::Text(child_text) = child_value {
                        link_text.push_str(child_text);
                    };
                }
                let text = NodeValue::Text(link_text);
                replace_node_value(node, text);
            };
        };
    };
    Ok(())
}

fn document_to_html<'a>(
    document: &'a AstNode<'a>,
    options: &ComrakOptions,
) -> Result<String, MarkdownError> {
    let mut output = vec![];
    format_html(document, options, &mut output)?;
    let html = String::from_utf8(output)?;
    Ok(html)
}

/// Removes extra soft breaks from a HTML document generated by comrak
fn fix_linebreaks(html: &str) -> String {
    html
        // Fix hardbreaks
        .replace("<br />\n", "<br>")
        // Remove extra soft breaks
        .replace(">\n<", "><")
        .trim_end_matches('\n')
        .to_string()
}

/// Markdown Lite
/// Supported features:
/// - bold and italic
/// - links and autolinks
/// - inline code and code blocks
pub fn markdown_lite_to_html(text: &str) -> Result<String, MarkdownError> {
    let options = build_comrak_options();
    let arena = Arena::new();
    let root = parse_document(&arena, text, &options);

    // Re-render blockquotes, headings, HRs, images and lists
    // Headings: poorly degrade on Pleroma
    // TODO: disable parser rules https://github.com/kivikakk/comrak/issues/244
    iter_nodes(root, &|node| {
        let node_value = node.data.borrow().value.clone();
        match node_value {
            // Blocks
            NodeValue::BlockQuote | NodeValue::Heading(_) | NodeValue::ThematicBreak => {
                // Replace children with paragraph containing markdown
                let mut markdown = node_to_markdown(node, &options)?;
                if matches!(node_value, NodeValue::BlockQuote) {
                    // Fix greentext
                    markdown = markdown.replace("> ", ">");
                };
                for child in node.children() {
                    child.detach();
                }
                let text = NodeValue::Text(markdown);
                let text_node = arena.alloc(create_node(text));
                node.append(text_node);
                replace_node_value(node, NodeValue::Paragraph);
            }
            NodeValue::Image(_) => replace_with_markdown(node, &options)?,
            NodeValue::List(_) => {
                // Replace list and list item nodes
                // while preserving their contents
                let mut replacements: Vec<&AstNode> = vec![];
                for list_item in node.children() {
                    let mut contents = vec![];
                    for paragraph in list_item.children() {
                        for content_node in paragraph.children() {
                            contents.push(content_node);
                        }
                        paragraph.detach();
                    }
                    let mut list_prefix_markdown = node_to_markdown(list_item, &options)?;
                    if let NodeValue::Item(item) = list_item.data.borrow().value {
                        if item.list_type == ListType::Ordered {
                            // Preserve numbering in ordered lists
                            let item_index_str = item.start.to_string();
                            list_prefix_markdown =
                                list_prefix_markdown.replace('1', &item_index_str);
                        };
                    };
                    if !replacements.is_empty() {
                        // Insert line break before next list item
                        let linebreak = NodeValue::LineBreak;
                        let linebreak_node = arena.alloc(create_node(linebreak));
                        replacements.push(linebreak_node);
                    };
                    let list_prefix = NodeValue::Text(list_prefix_markdown);
                    let list_prefix_node = arena.alloc(create_node(list_prefix));
                    replacements.push(list_prefix_node);
                    for content_node in contents {
                        replacements.push(content_node);
                    }
                    list_item.detach();
                }
                for child_node in replacements {
                    node.append(child_node);
                }
                replace_node_value(node, NodeValue::Paragraph);
            }
            NodeValue::Link(_) => fix_microsyntaxes(node)?,
            _ => (),
        };
        Ok(())
    })?;

    let html = document_to_html(root, &options)?;
    let html = fix_linebreaks(&html);
    Ok(html)
}

/// Markdown Basic
/// Supported features: links, linebreaks
pub fn markdown_basic_to_html(text: &str) -> Result<String, MarkdownError> {
    let options = build_comrak_options();
    let arena = Arena::new();
    let root = parse_document(&arena, text, &options);

    iter_nodes(root, &|node| {
        let node_value = node.data.borrow().value.clone();
        match node_value {
            NodeValue::Document
            | NodeValue::Text(_)
            | NodeValue::SoftBreak
            | NodeValue::LineBreak => (),
            NodeValue::Link(_) => fix_microsyntaxes(node)?,
            NodeValue::Paragraph => {
                if node.next_sibling().is_some() {
                    // If this is not the last paragraph,
                    // insert a line break, otherwise line break will not
                    // be preserved during HTML cleaning.
                    if let Some(last_child) = node.last_child() {
                        let last_child_value = &last_child.data.borrow().value;
                        if !matches!(last_child_value, NodeValue::LineBreak) {
                            let line_break = NodeValue::LineBreak;
                            let line_break_node = arena.alloc(create_node(line_break));
                            node.append(line_break_node);
                        };
                    };
                };
            }
            _ => replace_with_markdown(node, &options)?,
        };
        Ok(())
    })?;

    let html = document_to_html(root, &options)?;
    let html = fix_linebreaks(&html);
    Ok(html)
}

/// Full markdown
pub fn markdown_to_html(text: &str) -> String {
    let options = build_comrak_options();
    comrak::markdown_to_html(text, &options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_lite_to_html() {
        let text = "# heading\n\ntest **bold** test *italic* test ~~strike~~ with `code`, <span>html</span> and https://example.com\nnew line\n\ntwo new lines and a list:\n- item 1\n- item 2\n\n>greentext\n\n---\n\nimage: ![logo](logo.png)\n\ncode block:\n```\nlet test\ntest = 1\n```";
        let html = markdown_lite_to_html(text).unwrap();
        let expected_html = concat!(
            r#"<p># heading</p><p>test <strong>bold</strong> test <em>italic</em> test ~~strike~~ with <code>code</code>, &lt;span&gt;html&lt;/span&gt; and <a href="https://example.com">https://example.com</a><br>new line</p><p>two new lines and a list:</p><p>- item 1<br>- item 2</p><p>&gt;greentext</p><p>-----</p><p>image: ![logo](logo.png)</p><p>code block:</p>"#,
            "<pre><code>let test\ntest = 1\n</code></pre>",
        );
        assert_eq!(html, expected_html);
    }

    #[test]
    fn test_markdown_lite_to_html_ordered_list() {
        let text = "1. item 1\n2. item 2\n";
        let html = markdown_lite_to_html(text).unwrap();
        let expected_html = r#"<p>1.  item 1<br>2.  item 2</p>"#;
        assert_eq!(html, expected_html);
    }

    #[test]
    fn test_markdown_lite_to_html_mention() {
        let text = "@user@example.org test";
        let html = markdown_lite_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_lite_to_html_hashtag() {
        let text = "#hashtag test";
        let html = markdown_lite_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_lite_to_html_object_link() {
        let text = "[[https://example.org/objects/1]] test";
        let html = markdown_lite_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_basic_to_html() {
        let text = "test **bold** test *italic* test ~~strike~~ with `code`, <span>html</span> and https://example.com\nnew line\n\nanother line";
        let html = markdown_basic_to_html(text).unwrap();
        let expected_html = concat!(
            "<p>",
            "test **bold** test *italic* test ~~strike~~ with `code`, &lt;span&gt;html&lt;/span&gt;",
            r#" and <a href="https://example.com">https://example.com</a>"#,
            "<br>new line<br></p>",
            "<p>another line</p>",
        );
        assert_eq!(html, expected_html);
    }

    #[test]
    fn test_markdown_basic_to_html_mention() {
        let text = "@user@example.org test";
        let html = markdown_basic_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_to_html() {
        let text = "# heading\n\ntest";
        let html = markdown_to_html(text);
        assert_eq!(html, "<h1>heading</h1>\n<p>test</p>\n",);
    }
}
