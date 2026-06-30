use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders};
use smails_core::{Attachment, preview_text};

pub(crate) struct ParsedMail {
    pub(crate) from_name: String,
    pub(crate) subject: String,
    pub(crate) preview: String,
    pub(crate) html: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) attachments: Vec<Attachment>,
}

pub(crate) fn parse_mail(raw: &[u8], fallback_from: &str) -> ParsedMail {
    let Some(message) = MessageParser::default().parse(raw) else {
        return empty_mail(fallback_from);
    };
    let from_name = message
        .from()
        .and_then(|from| from.first())
        .and_then(|from| from.name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback_from.to_owned());
    let subject = message
        .subject()
        .filter(|subject| !subject.is_empty())
        .unwrap_or("(no subject)")
        .to_owned();
    let html = first_body(message.html_bodies());
    let text = first_body(message.text_bodies());
    let preview = text
        .as_deref()
        .or(html.as_deref())
        .map(preview_text)
        .unwrap_or_default();
    ParsedMail {
        from_name,
        subject,
        preview,
        html,
        text,
        attachments: attachments(&message),
    }
}

fn empty_mail(fallback_from: &str) -> ParsedMail {
    ParsedMail {
        from_name: fallback_from.to_owned(),
        subject: "(no subject)".to_owned(),
        preview: String::new(),
        html: None,
        text: None,
        attachments: Vec::new(),
    }
}

fn first_body<'a>(parts: impl Iterator<Item = &'a MessagePart<'a>>) -> Option<String> {
    parts
        .filter_map(|part| part.text_contents())
        .find(|body| !body.trim().is_empty())
        .map(str::to_owned)
}

fn attachments(message: &Message<'_>) -> Vec<Attachment> {
    message
        .attachments()
        .enumerate()
        .map(|(index, part)| attachment_metadata(index, part))
        .collect()
}

fn attachment_metadata(index: usize, part: &MessagePart<'_>) -> Attachment {
    Attachment {
        index,
        filename: part.attachment_name().map(str::to_owned),
        content_type: content_type(part),
        content_id: part.content_id().map(clean_content_id),
        disposition: part
            .content_disposition()
            .map(|disposition| disposition.ctype().to_ascii_lowercase()),
        size: part.len(),
    }
}

fn content_type(part: &MessagePart<'_>) -> String {
    part.content_type()
        .map(|content_type| match content_type.subtype() {
            Some(subtype) => format!("{}/{}", content_type.ctype(), subtype),
            None => content_type.ctype().to_owned(),
        })
        .unwrap_or_else(|| "application/octet-stream".to_owned())
}

fn clean_content_id(value: &str) -> String {
    value.trim().trim_matches('<').trim_matches('>').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_message_display_and_body() {
        let raw =
            b"From: Alice <a@example.com>\r\nSubject: Hi\r\nContent-Type: text/plain\r\n\r\nhello";
        let parsed = parse_mail(raw, "fallback@example.com");

        assert_eq!(parsed.from_name, "Alice");
        assert_eq!(parsed.subject, "Hi");
        assert_eq!(parsed.preview, "hello");
        assert_eq!(parsed.text.as_deref(), Some("hello"));
    }

    #[test]
    fn parses_multipart_text_html_and_attachment_metadata() {
        let raw = concat!(
            "From: Alice <a@example.com>\r\n",
            "Subject: Files\r\n",
            "Content-Type: multipart/mixed; boundary=\"mixed\"\r\n\r\n",
            "--mixed\r\n",
            "Content-Type: multipart/alternative; boundary=\"alt\"\r\n\r\n",
            "--alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nplain\r\n",
            "--alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<b>html</b>\r\n",
            "--alt--\r\n",
            "--mixed\r\n",
            "Content-Type: text/plain; name=\"code.txt\"\r\n",
            "Content-Disposition: attachment; filename=\"code.txt\"\r\n",
            "Content-ID: <file-1>\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\n",
            "aGVsbG8=\r\n",
            "--mixed--\r\n"
        );
        let parsed = parse_mail(raw.as_bytes(), "fallback@example.com");

        assert_eq!(parsed.from_name, "Alice");
        assert_eq!(parsed.subject, "Files");
        assert_eq!(parsed.text.as_deref(), Some("plain"));
        assert_eq!(parsed.html.as_deref(), Some("<b>html</b>"));
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].index, 0);
        assert_eq!(parsed.attachments[0].filename.as_deref(), Some("code.txt"));
        assert_eq!(parsed.attachments[0].content_type, "text/plain");
        assert_eq!(parsed.attachments[0].content_id.as_deref(), Some("file-1"));
        assert_eq!(
            parsed.attachments[0].disposition.as_deref(),
            Some("attachment")
        );
        assert_eq!(parsed.attachments[0].size, 5);
    }
}
