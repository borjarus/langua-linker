//! Anki fields are HTML. LanguaLinker stores plain NFC text, so fields are flattened: block tags
//! and `<br>` become line breaks, other tags are dropped, entities are decoded, and sound/image
//! references are removed (media is not converted; the report counts them).

use langualinker_domain::text::nfc;

/// Plain text of an Anki field plus what was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Flattened {
    pub text: String,
    /// The field contained tags (formatting was lost).
    pub had_markup: bool,
    /// `[sound:…]` and `<img>` references removed.
    pub media_refs: usize,
}

const BLOCK_TAGS: &[&str] = &[
    "br", "div", "p", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "hr",
];

/// Flattens a field. `single_line` joins lines with a space (fronts are one line).
pub fn flatten(html: &str, single_line: bool) -> Flattened {
    let mut out = String::with_capacity(html.len());
    let mut result = Flattened::default();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("[sound:") {
            result.media_refs += 1;
            rest = after.find(']').map_or("", |end| &after[end + 1..]);
            continue;
        }
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else {
                // A lone '<' is text ("x < y").
                out.push('<');
                rest = &rest[1..];
                continue;
            };
            let tag = &rest[1..end];
            result.had_markup = true;
            let name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            if name == "img" {
                result.media_refs += 1;
            }
            if name == "style" || name == "script" {
                // Skip the element's content too.
                let close = format!("</{name}");
                let lower = rest.to_ascii_lowercase();
                rest = match lower.find(&close) {
                    Some(start) => rest[start..]
                        .find('>')
                        .map_or("", |e| &rest[start + e + 1..]),
                    None => "",
                };
                continue;
            }
            if BLOCK_TAGS.contains(&name.as_str()) {
                out.push('\n');
            }
            rest = &rest[end + 1..];
            continue;
        }
        if rest.starts_with('&')
            && let Some((decoded, used)) = entity(rest)
        {
            out.push(decoded);
            rest = &rest[used..];
            continue;
        }
        let ch = rest.chars().next().expect("non-empty");
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    let normalized = nfc(&out
        .replace('\u{a0}', " ")
        .replace("\r\n", "\n")
        .replace('\r', "\n"));
    let lines: Vec<String> = normalized
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    result.text = lines.join(if single_line { " " } else { "\n" });
    result
}

/// Decodes one entity at the start of `text`; returns the character and the bytes used.
fn entity(text: &str) -> Option<(char, usize)> {
    let end = text[..text.len().min(12)].find(';')?;
    let body = &text[1..end];
    let decoded = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" | "#39" => '\'',
        "nbsp" => ' ',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "laquo" => '«',
        "raquo" => '»',
        "bdquo" => '„',
        "rdquo" => '”',
        "ldquo" => '“',
        "rsquo" => '’',
        "lsquo" => '‘',
        _ => {
            let number = body.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((decoded, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_become_text_and_lines() {
        let flat = flatten(
            "<div><b>der</b> Hund</div><div>pies&nbsp;(zwierzę)</div>",
            false,
        );
        assert_eq!(flat.text, "der Hund\npies (zwierzę)");
        assert!(flat.had_markup);
        assert_eq!(flatten("a<br>b<br/>c", true).text, "a b c");
    }

    #[test]
    fn entities_are_decoded_and_lone_brackets_kept() {
        assert_eq!(
            flatten("Tom &amp; Jerry &#8222;x&#x201D;", true).text,
            "Tom & Jerry „x”"
        );
        assert_eq!(flatten("x < y", true).text, "x < y");
        assert_eq!(flatten("&unknown; stays", true).text, "&unknown; stays");
    }

    #[test]
    fn media_references_are_removed_and_counted() {
        let flat = flatten("Hund [sound:hund.mp3]<img src=\"hund.jpg\">", true);
        assert_eq!(flat.text, "Hund");
        assert_eq!(flat.media_refs, 2);
    }

    #[test]
    fn style_and_script_content_is_dropped() {
        assert_eq!(
            flatten(
                "<style>.x{color:red}</style>Haus<script>alert(1)</script>",
                true
            )
            .text,
            "Haus"
        );
    }

    #[test]
    fn text_is_nfc() {
        assert_eq!(flatten("z\u{0307}ółw", true).text, "żółw");
    }
}
