use smails_core::{MessageDetail, MessageSummary, format_bytes, initials, short_id};
use yew::prelude::*;

use crate::ws::WsStatus;

#[derive(Properties, PartialEq)]
pub struct ShellProps {
    pub children: Children,
}

#[function_component(Shell)]
pub fn shell(props: &ShellProps) -> Html {
    html! {
        <div class="page">
            <header class="topbar">
                <a class="brand" href="/">{"smails"}</a>
                <nav>
                    <a href="/#inbox">{"Inbox"}</a>
                    <a href="/mcp">{"MCP"}</a>
                    <a href="/email-api">{"API"}</a>
                    <a href="/otp">{"OTP"}</a>
                </nav>
            </header>
            <main>{ for props.children.iter() }</main>
            <footer>{"Disposable email for humans and agents. No signup."}</footer>
        </div>
    }
}

#[function_component(Hero)]
pub fn hero() -> Html {
    html! {
        <section id="top" class="hero">
            <p class="eyebrow">{"Agent-native - No signup - Free"}</p>
            <h1>{"Disposable email for humans and agents."}</h1>
            <p>{"An instant throwaway inbox for sign-ups, codes, and confirmations with a REST API, CLI, and MCP server."}</p>
            <a class="primary" href="#inbox">{"Get my inbox"}</a>
        </section>
    }
}

#[derive(Properties, PartialEq)]
pub struct InboxProps {
    pub address: String,
    pub messages: Vec<MessageSummary>,
    pub selected_id: Option<String>,
    pub detail: Option<MessageDetail>,
    pub loaded: bool,
    pub ws_status: WsStatus,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub on_new: Callback<MouseEvent>,
    pub on_refresh: Callback<MouseEvent>,
    pub on_copy: Callback<MouseEvent>,
    pub on_open: Callback<String>,
    pub on_delete: Callback<String>,
    pub on_back: Callback<MouseEvent>,
}

#[function_component(Inbox)]
pub fn inbox(props: &InboxProps) -> Html {
    let unread = props
        .messages
        .iter()
        .filter(|message| message.read == 0)
        .count();
    let selected = props
        .selected_id
        .as_ref()
        .and_then(|id| props.messages.iter().find(|message| &message.id == id));

    html! {
        <section id="inbox" class="inbox-section">
            <div class="inbox-card">
                <div class="inbox-header">
                    <div class="address-block">
                        <span>{"Your live inbox"}</span>
                        <code>{ if props.address.is_empty() { " ".to_owned() } else { props.address.clone() } }</code>
                    </div>
                    <div class="actions">
                        <button type="button" title="New address" onclick={props.on_new.clone()}>{"+"}</button>
                        <button type="button" title="Refresh" onclick={props.on_refresh.clone()}>{"Refresh"}</button>
                        <button type="button" title="Copy" onclick={props.on_copy.clone()}>{"Copy"}</button>
                    </div>
                </div>

                <div class="status-row">
                    <span>{format!("{} messages", props.messages.len())}{ if unread > 0 { format!(" - {unread} unread") } else { String::new() } }</span>
                    <span class={classes!("live-dot", ws_class(props.ws_status))}>{ws_label(props.ws_status)}</span>
                </div>

                if let Some(notice) = &props.notice {
                    <div class="notice">{notice}</div>
                }
                if let Some(error) = &props.error {
                    <div class="error">{error}</div>
                }

                <div class="mail-layout">
                    <aside class={classes!("message-list", selected.is_some().then_some("is-hidden-mobile"))}>
                        if !props.loaded {
                            <div class="empty">{"Loading inbox..."}</div>
                        } else if props.messages.is_empty() {
                            <div class="empty">
                                <strong>{"Nothing here yet"}</strong>
                                <span>{"New mail lands here on its own."}</span>
                            </div>
                        } else {
                            <ul>
                                { for props.messages.iter().map(|message| {
                                    html! {
                                        <MessageRow
                                            message={message.clone()}
                                            active={props.selected_id.as_deref() == Some(message.id.as_str())}
                                            on_open={props.on_open.clone()}
                                        />
                                    }
                                })}
                            </ul>
                        }
                    </aside>
                    <section class={classes!("message-detail", selected.is_none().then_some("is-hidden-mobile"))}>
                        if let Some(message) = selected {
                            <MessageDetailView
                                message={message.clone()}
                                detail={props.detail.clone()}
                                on_delete={props.on_delete.clone()}
                                on_back={props.on_back.clone()}
                            />
                        } else {
                            <div class="empty detail-empty">{"Select a message to read it."}</div>
                        }
                    </section>
                </div>
            </div>
        </section>
    }
}

#[derive(Properties, PartialEq)]
struct MessageRowProps {
    message: MessageSummary,
    active: bool,
    on_open: Callback<String>,
}

#[function_component(MessageRow)]
fn message_row(props: &MessageRowProps) -> Html {
    let id = props.message.id.clone();
    let onclick = {
        let on_open = props.on_open.clone();
        Callback::from(move |_| on_open.emit(id.clone()))
    };
    html! {
        <li>
            <button type="button" class={classes!("message-row", props.active.then_some("active"))} {onclick}>
                <span class="avatar">{initials(&props.message.from_name)}</span>
                <span class="message-meta">
                    <span class="message-from">{&props.message.from_name}</span>
                    <span class="message-subject">{&props.message.subject}</span>
                    <span class="message-preview">{&props.message.preview}</span>
                </span>
                <span class="message-side">
                    <span>{short_id(&props.message.id)}</span>
                    if props.message.read == 0 {
                        <span class="unread" aria-label="Unread"></span>
                    }
                </span>
            </button>
        </li>
    }
}

#[derive(Properties, PartialEq)]
struct MessageDetailProps {
    message: MessageSummary,
    detail: Option<MessageDetail>,
    on_delete: Callback<String>,
    on_back: Callback<MouseEvent>,
}

#[function_component(MessageDetailView)]
fn message_detail(props: &MessageDetailProps) -> Html {
    let id = props.message.id.clone();
    let ondelete = {
        let on_delete = props.on_delete.clone();
        Callback::from(move |_| on_delete.emit(id.clone()))
    };
    html! {
        <div class="detail">
            <div class="detail-title">
                <button type="button" class="back" onclick={props.on_back.clone()}>{"Back"}</button>
                <h2>{&props.message.subject}</h2>
                <button type="button" class="danger" onclick={ondelete}>{"Delete"}</button>
            </div>
            <div class="detail-from">
                <span class="avatar">{initials(&props.message.from_name)}</span>
                <span>
                    <strong>{&props.message.from_name}</strong>
                    <small>{&props.message.from_addr}</small>
                </span>
            </div>
            if let Some(detail) = &props.detail {
                if !detail.attachments.is_empty() {
                    <div class="attachments">
                        { for detail.attachments.iter().map(|attachment| {
                            html! {
                                <div class="attachment">
                                    <span class="attachment-info">
                                        <strong>{attachment.filename.as_deref().unwrap_or("attachment")}</strong>
                                        <span>{format!("{} - {}", attachment.content_type, format_bytes(attachment.size))}</span>
                                    </span>
                                </div>
                            }
                        })}
                    </div>
                }
                if let Some(html_body) = &detail.html {
                    <iframe
                        class="message-frame"
                        sandbox=""
                        srcdoc={format!("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data: cid:;\">{html_body}")}
                        title="Message content"
                    />
                } else {
                    <pre class="message-body">{detail.text.clone().unwrap_or_else(|| "This message has no content.".to_owned())}</pre>
                }
            } else {
                <div class="empty">{"Loading message..."}</div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct CodeBlockProps {
    pub label: &'static str,
    pub code: AttrValue,
}

#[function_component(CodeBlock)]
pub fn code_block(props: &CodeBlockProps) -> Html {
    html! {
        <div class="code-block">
            <span>{props.label}</span>
            <pre>{props.code.clone()}</pre>
        </div>
    }
}

#[function_component(AgentDocs)]
pub fn agent_docs() -> Html {
    html! {
        <section id="agents" class="docs-grid">
            <div>
                <p class="eyebrow">{"For agents & CLI"}</p>
                <h2>{"Give your agent its own inbox."}</h2>
                <p>{"Humans and agents share the same mailbox. Drive it from the terminal, REST API, or MCP."}</p>
            </div>
            <pre>{"smails create\nsmails inbox\nsmails read <id>"}</pre>
            <pre>{"{\n  \"mcpServers\": {\n    \"smails\": { \"command\": \"smails\", \"args\": [\"mcp\"] }\n  }\n}"}</pre>
            <a class="text-link" href="/mcp">{"Read the MCP server guide"}</a>
        </section>
    }
}

#[function_component(ApiDocs)]
pub fn api_docs() -> Html {
    html! {
        <section id="api" class="docs-grid">
            <div>
                <p class="eyebrow">{"REST API"}</p>
                <h2>{"Or just call the API."}</h2>
                <p>{"Create a mailbox, then poll or stream messages with the returned token."}</p>
            </div>
            <pre>{"POST /api/mailbox\nGET  /api/mailbox/messages\nGET  /api/mailbox/messages/:id\nDEL  /api/mailbox/messages/:id\nWS   /api/mailbox/connect"}</pre>
            <a class="text-link" href="/email-api">{"Read the REST API reference"}</a>
        </section>
    }
}

#[function_component(Faq)]
pub fn faq() -> Html {
    html! {
        <section id="faq" class="faq">
            <h2>{"Everything you might be wondering."}</h2>
            <details open=true><summary>{"Is it free?"}</summary><p>{"Completely. No account, no paywall, no card."}</p></details>
            <details><summary>{"Can agents use it?"}</summary><p>{"Yes. Use the CLI, REST API, or MCP server."}</p></details>
            <details><summary>{"Can I reply or send mail?"}</summary><p>{"Not yet. smails is receive-only."}</p></details>
        </section>
    }
}

fn ws_label(status: WsStatus) -> &'static str {
    match status {
        WsStatus::Connecting => "Connecting",
        WsStatus::Connected => "Live",
        WsStatus::Disconnected => "Offline",
    }
}

fn ws_class(status: WsStatus) -> &'static str {
    match status {
        WsStatus::Connecting => "connecting",
        WsStatus::Connected => "connected",
        WsStatus::Disconnected => "disconnected",
    }
}
