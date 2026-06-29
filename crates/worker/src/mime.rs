use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use smails_core::preview_text;

pub(crate) struct DisplayFields {
    pub(crate) from_name: String,
    pub(crate) subject: String,
    pub(crate) preview: String,
}

#[derive(Default)]
pub(crate) struct BodyParts {
    pub(crate) html: Option<String>,
    pub(crate) text: Option<String>,
}

fn split_headers_body(raw: &str) -> (&str, &str) {
    if let Some(index) = raw.find("\r\n\r\n") {
        return (&raw[..index], &raw[index + 4..]);
    }
    if let Some(index) = raw.find("\n\n") {
        return (&raw[..index], &raw[index + 2..]);
    }
    ("", raw)
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    let mut found = false;
    let mut value = String::new();

    for line in headers.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if found {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }

        if found {
            break;
        }

        let Some((key, line_value)) = line.split_once(':') else {
            continue;
        };
        if key.eq_ignore_ascii_case(name) {
            found = true;
            value = line_value.trim().to_owned();
        }
    }

    found.then_some(value)
}

fn boundary_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').skip(1).find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case("boundary")
            .then(|| value.trim().trim_matches('"').to_owned())
    })
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_quoted_printable(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'=' {
            if bytes.get(index + 1) == Some(&b'\r') && bytes.get(index + 2) == Some(&b'\n') {
                index += 3;
                continue;
            }
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 2;
                continue;
            }
            if let (Some(left), Some(right)) = (
                bytes.get(index + 1).and_then(|byte| hex_value(*byte)),
                bytes.get(index + 2).and_then(|byte| hex_value(*byte)),
            ) {
                out.push((left << 4) | right);
                index += 3;
                continue;
            }
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

fn decode_transfer(headers: &str, body: &str) -> String {
    match header_value(headers, "content-transfer-encoding")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "base64" => {
            let compact: String = body.chars().filter(|char| !char.is_whitespace()).collect();
            BASE64
                .decode(compact)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default()
        }
        "quoted-printable" => decode_quoted_printable(body),
        _ => body.to_owned(),
    }
}

pub(crate) fn parse_body_parts(raw: &str) -> BodyParts {
    parse_body_parts_at_depth(raw, 0)
}

fn parse_body_parts_at_depth(raw: &str, depth: usize) -> BodyParts {
    let (headers, body) = split_headers_body(raw);
    let content_type = header_value(headers, "content-type").unwrap_or_default();
    let lower_content_type = content_type.to_ascii_lowercase();

    if depth < 4
        && lower_content_type.starts_with("multipart/")
        && let Some(boundary) = boundary_from_content_type(&content_type)
    {
        let marker = format!("--{boundary}");
        let mut parts = BodyParts::default();

        for chunk in body.split(&marker).skip(1) {
            let chunk = chunk.trim_start_matches("\r\n").trim_start_matches('\n');
            if chunk.starts_with("--") {
                break;
            }

            let child = parse_body_parts_at_depth(chunk.trim(), depth + 1);
            if parts.text.is_none() {
                parts.text = child.text;
            }
            if parts.html.is_none() {
                parts.html = child.html;
            }
            if parts.text.is_some() && parts.html.is_some() {
                break;
            }
        }

        return parts;
    }

    let body = decode_transfer(headers, body).trim().to_owned();
    if body.is_empty() {
        return BodyParts::default();
    }

    if lower_content_type.contains("text/html") {
        BodyParts {
            html: Some(body),
            text: None,
        }
    } else {
        BodyParts {
            html: None,
            text: Some(body),
        }
    }
}

fn name_from_from_header(header: &str) -> Option<String> {
    let name = header
        .split('<')
        .next()
        .unwrap_or(header)
        .trim()
        .trim_matches('"');
    (!name.is_empty()).then(|| name.to_owned())
}

pub(crate) fn display_fields(raw: &str, fallback_from: &str) -> DisplayFields {
    let (headers, _) = split_headers_body(raw);
    let from_header = header_value(headers, "from").unwrap_or_else(|| fallback_from.to_owned());
    let from_name = name_from_from_header(&from_header).unwrap_or_else(|| fallback_from.to_owned());
    let subject = header_value(headers, "subject")
        .filter(|subject| !subject.is_empty())
        .unwrap_or_else(|| "(no subject)".to_owned());
    let parts = parse_body_parts(raw);
    let preview = parts
        .text
        .as_deref()
        .or(parts.html.as_deref())
        .map(preview_text)
        .unwrap_or_default();

    DisplayFields {
        from_name,
        subject,
        preview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_message_body() {
        let raw =
            "From: Alice <a@example.com>\r\nSubject: Hi\r\nContent-Type: text/plain\r\n\r\nhello";
        let fields = display_fields(raw, "fallback@example.com");
        let parts = parse_body_parts(raw);

        assert_eq!(fields.from_name, "Alice");
        assert_eq!(fields.subject, "Hi");
        assert_eq!(fields.preview, "hello");
        assert_eq!(parts.text.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_multipart_text_and_html() {
        let raw = concat!(
            "Content-Type: multipart/alternative; boundary=\"x\"\r\n\r\n",
            "--x\r\nContent-Type: text/plain\r\n\r\nplain\r\n",
            "--x\r\nContent-Type: text/html\r\n\r\n<b>html</b>\r\n",
            "--x--\r\n"
        );
        let parts = parse_body_parts(raw);

        assert_eq!(parts.text.as_deref(), Some("plain"));
        assert_eq!(parts.html.as_deref(), Some("<b>html</b>"));
    }

    #[test]
    fn decodes_common_transfer_encodings() {
        assert_eq!(
            decode_transfer("Content-Transfer-Encoding: base64", "aGVsbG8="),
            "hello"
        );
        assert_eq!(
            decode_transfer(
                "Content-Transfer-Encoding: quoted-printable",
                "hello=0Aworld"
            ),
            "hello\nworld"
        );
    }
}
