use comrak::{
    format_commonmark,
    format_html,
    nodes::{Ast, AstNode, ListType, NodeValue},
    parse_document,
    Arena,
    ComrakOptions,
    ComrakExtensionOptions,
    ComrakParseOptions,
    ComrakRenderOptions,
};

#[derive(thiserror::Error, Debug)]
pub enum MarkdownError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

fn iter_nodes<'a, F>(
    node: &'a AstNode<'a>,
    func: &F,
) -> Result<(), MarkdownError>
    where F: Fn(&'a AstNode<'a>) -> Result<(), MarkdownError>
{
    func(node)?;
    for child in node.children() {
        iter_nodes(child, func)?;
    };
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

/// Supported markdown features:
/// - bold and italic
/// - links and autolinks
/// - inline code and code blocks
pub fn markdown_to_html(text: &str) -> Result<String, MarkdownError> {
    let options = ComrakOptions {
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
    };
    let arena = Arena::new();
    let root = parse_document(
        &arena,
        text,
        &options,
    );

    // Re-render blockquotes, headings, HRs, images and lists
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
                };
                let text = NodeValue::Text(markdown.as_bytes().to_vec());
                let text_node = arena.alloc(AstNode::from(text));
                node.append(text_node);
                let mut borrowed_node = node.data.borrow_mut();
                *borrowed_node = Ast::new(NodeValue::Paragraph);
            },
            // Inlines
            NodeValue::Image(_) => {
                // Replace node with text node containing markdown
                let markdown = node_to_markdown(node, &options)?;
                for child in node.children() {
                    child.detach();
                };
                let text = NodeValue::Text(markdown.as_bytes().to_vec());
                let mut borrowed_node = node.data.borrow_mut();
                *borrowed_node = Ast::new(text);
            },
            NodeValue::List(_) => {
                // Replace list and list item nodes
                // while preserving their contents
                let mut replacements: Vec<&AstNode> = vec![];
                for list_item in node.children() {
                    let mut contents = vec![];
                    for paragraph in list_item.children() {
                        for content_node in paragraph.children() {
                            contents.push(content_node);
                        };
                        paragraph.detach();
                    };
                    let mut list_prefix_markdown =
                        node_to_markdown(list_item, &options)?;
                    if let NodeValue::Item(item) = list_item.data.borrow().value {
                        if item.list_type == ListType::Ordered {
                            // Preserve numbering in ordered lists
                            let item_index_str = item.start.to_string();
                            list_prefix_markdown =
                                list_prefix_markdown.replace('1', &item_index_str);
                        };
                    };
                    let list_prefix =
                        NodeValue::Text(list_prefix_markdown.as_bytes().to_vec());
                    if !replacements.is_empty() {
                        // Insert line break before next list item
                        let linebreak = NodeValue::LineBreak;
                        replacements.push(arena.alloc(AstNode::from(linebreak)));
                    };
                    replacements.push(arena.alloc(AstNode::from(list_prefix)));
                    for content_node in contents {
                        replacements.push(content_node);
                    };
                    list_item.detach();
                };
                for child_node in replacements {
                    node.append(child_node);
                };
                let mut borrowed_node = node.data.borrow_mut();
                *borrowed_node = Ast::new(NodeValue::Paragraph);
            },
            NodeValue::Link(_) => {
                if let Some(prev) = node.previous_sibling() {
                    if let NodeValue::Text(ref prev_text) = prev.data.borrow().value {
                        let prev_text = String::from_utf8(prev_text.to_vec())?;
                        // Remove autolink if mention or object link syntax is found
                        if prev_text.ends_with('@') || prev_text.ends_with("[[") {
                            let mut link_text = vec![];
                            for child in node.children() {
                                child.detach();
                                let child_value = &child.data.borrow().value;
                                if let NodeValue::Text(child_text) = child_value {
                                    link_text.extend(child_text);
                                };
                            };
                            let text = NodeValue::Text(link_text);
                            let mut borrowed_node = node.data.borrow_mut();
                            *borrowed_node = Ast::new(text);
                        };
                    };
                };
            },
            _ => (),
        };
        Ok(())
    })?;

    let mut output = vec![];
    format_html(root, &options, &mut output)?;
    let html = String::from_utf8(output)?
        // Fix hardbreaks
        .replace("<br />\n", "<br>")
        // Remove extra soft breaks
        .replace(">\n<", "><")
        .trim_end_matches('\n')
        .to_string();
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_to_html() {
        let text = "# heading\n\ntest **bold** test *italic* test ~~strike~~ with `code`, <span>html</span> and https://example.com\nnew line\n\ntwo new lines and a list:\n- item 1\n- item 2\n\n>greentext\n\n---\n\nimage: ![logo](logo.png)\n\ncode block:\n```\nlet test\ntest = 1\n```";
        let html = markdown_to_html(text).unwrap();
        let expected_html = concat!(
            r#"<p># heading</p><p>test <strong>bold</strong> test <em>italic</em> test ~~strike~~ with <code>code</code>, &lt;span&gt;html&lt;/span&gt; and <a href="https://example.com">https://example.com</a><br>new line</p><p>two new lines and a list:</p><p>- item 1<br>- item 2</p><p>&gt;greentext</p><p>-----</p><p>image: ![logo](logo.png)</p><p>code block:</p>"#,
            "<pre><code>let test\ntest = 1\n</code></pre>",
        );
        assert_eq!(html, expected_html);
    }

    #[test]
    fn test_markdown_to_html_ordered_list() {
        let text = "1. item 1\n2. item 2\n";
        let html = markdown_to_html(text).unwrap();
        let expected_html = r#"<p>1.  item 1<br>2.  item 2</p>"#;
        assert_eq!(html, expected_html);
    }

    #[test]
    fn test_markdown_to_html_mention() {
        let text = "@user@example.org test";
        let html = markdown_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_to_html_hashtag() {
        let text = "#hashtag test";
        let html = markdown_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }

    #[test]
    fn test_markdown_to_html_object_link() {
        let text = "[[https://example.org/objects/1]] test";
        let html = markdown_to_html(text).unwrap();
        assert_eq!(html, format!("<p>{}</p>", text));
    }
}
