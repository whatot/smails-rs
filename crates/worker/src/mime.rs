use mail_parser::{Message, MessageParser, MessagePart, MimeHeaders};
use smails_core::{Attachment, preview_text};

pub(crate) struct DisplayFields {
    pub(crate) from_name: String,
    pub(crate) subject: String,
    pub(crate) preview: String,
}

#[derive(Default)]
pub(crate) struct ParsedMail {
    pub(crate) html: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) attachments: Vec<Attachment>,
}

pub(crate) fn parse_mail(raw: &[u8]) -> ParsedMail {
    MessageParser::default()
        .parse(raw)
        .map(|message| ParsedMail {
            html: first_body(message.html_bodies()),
            text: first_body(message.text_bodies()),
            attachments: attachments(&message),
        })
        .unwrap_or_default()
}

pub(crate) fn display_fields(raw: &[u8], fallback_from: &str) -> DisplayFields {
    let message = MessageParser::default().parse(raw);
    let from_name = message
        .as_ref()
        .and_then(|message| message.from())
        .and_then(|from| from.first())
        .and_then(|from| from.name.as_deref())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| fallback_from.to_owned());
    let subject = message
        .as_ref()
        .and_then(|message| message.subject())
        .filter(|subject| !subject.is_empty())
        .unwrap_or("(no subject)")
        .to_owned();
    let parts = message
        .as_ref()
        .map(|message| ParsedMail {
            html: first_body(message.html_bodies()),
            text: first_body(message.text_bodies()),
            attachments: Vec::new(),
        })
        .unwrap_or_default();
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
        let fields = display_fields(raw, "fallback@example.com");
        let parsed = parse_mail(raw);

        assert_eq!(fields.from_name, "Alice");
        assert_eq!(fields.subject, "Hi");
        assert_eq!(fields.preview, "hello");
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
        let parsed = parse_mail(raw.as_bytes());

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
