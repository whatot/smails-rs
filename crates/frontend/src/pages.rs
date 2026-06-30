use smails_core::DEFAULT_BASE_URL;
use web_sys::window;
use yew::prelude::*;

use crate::components::{CodeBlock, Shell};

#[function_component(McpPage)]
pub fn mcp_page() -> Html {
    html! {
        <Shell>
            <article class="content-page">
                <header class="content-hero">
                    <p class="eyebrow">{"MCP - Free - No API key - No signup"}</p>
                    <h1>{"A disposable email MCP server for your AI agent."}</h1>
                    <p>{"Give Claude, Cursor, Codex, or any MCP client a receive-only inbox for verification codes and magic links."}</p>
                    <div class="content-actions">
                        <a class="primary" href="#setup">{"Add the server"}</a>
                        <a class="secondary" href="/">{"Try the web inbox"}</a>
                    </div>
                </header>

                <section class="content-section">
                    <h2>{"Why this one"}</h2>
                    <ul class="check-list">
                        <li>{"No API key. The agent provisions its own mailbox at runtime."}</li>
                        <li>{"No signup, no account, no card."}</li>
                        <li>{"Same mailbox across MCP, CLI, REST API, and the web."}</li>
                        <li>{"Runs over stdio, so there is nothing to host."}</li>
                    </ul>
                </section>

                <section id="setup" class="content-section">
                    <h2>{"Set it up"}</h2>
                    <p>{"Use the Rust CLI binary as the MCP stdio server."}</p>
                    <CodeBlock label="terminal" code={"smails mcp"} />
                    <CodeBlock label="mcp.json" code={r#"{
  "mcpServers": {
    "smails": {
      "command": "smails",
      "args": ["mcp"]
    }
  }
}"#} />
                </section>

                <section class="content-section">
                    <h2>{"Tools the agent gets"}</h2>
                    <div class="endpoint-list">
                        <Endpoint method="tool" path="create_mailbox" desc="Create a fresh disposable address." />
                        <Endpoint method="tool" path="get_address" desc="Return the current mailbox address." />
                        <Endpoint method="tool" path="list_messages" desc="List sender, subject, preview, and read state." />
                        <Endpoint method="tool" path="read_message" desc="Read the full parsed body." />
                        <Endpoint method="tool" path="delete_message" desc="Delete a message when done." />
                    </div>
                </section>

                <section class="content-section">
                    <h2>{"How the flow works"}</h2>
                    <ol class="step-list">
                        <li>{"Agent calls create_mailbox and gets an address."}</li>
                        <li>{"It enters the address into the sign-up form."}</li>
                        <li>{"smails receives the verification email."}</li>
                        <li>{"Agent calls list_messages, then read_message."}</li>
                        <li>{"It extracts the code or magic link and finishes the flow."}</li>
                    </ol>
                </section>
            </article>
        </Shell>
    }
}

#[function_component(ApiPage)]
pub fn api_page() -> Html {
    let base_url = site_base_url();
    let ws_url = websocket_base_url(&base_url);
    let curl_quickstart = format!(
        r#"# create a mailbox
curl -X POST {base_url}/api/mailbox

# list messages
curl {base_url}/api/mailbox/messages \
  -H "Authorization: Bearer <token>"

# read one message
curl {base_url}/api/mailbox/messages/<id> \
    -H "Authorization: Bearer <token>""#
    );
    let js_quickstart = format!(
        r#"const ws = new WebSocket(
  "{ws_url}/api/mailbox/connect?token=" + token
);
ws.onmessage = (event) => {{
  const msg = JSON.parse(event.data);
  if (msg.type === "new_message") fetchMessages();
}};"#
    );

    html! {
        <Shell>
            <article class="content-page">
                <header class="content-hero">
                    <p class="eyebrow">{"REST API - Free - No API key - No signup"}</p>
                    <h1>{"A disposable email REST API, no key required."}</h1>
                    <p>{"Create a mailbox, read incoming messages, and stream new-mail notifications from scripts or agents."}</p>
                    <div class="content-actions">
                        <a class="primary" href="#quickstart">{"Quick start"}</a>
                        <a class="secondary" href="/mcp">{"Prefer MCP?"}</a>
                    </div>
                </header>

                <section id="quickstart" class="content-section">
                    <h2>{"Quick start"}</h2>
                    <CodeBlock label="curl" code={AttrValue::from(curl_quickstart)} />
                    <CodeBlock label="javascript" code={AttrValue::from(js_quickstart)} />
                </section>

                <section class="content-section">
                    <h2>{"Endpoints"}</h2>
                    <div class="endpoint-list">
                        <Endpoint method="POST" path="/api/mailbox" desc="Create a mailbox and return address plus token." />
                        <Endpoint method="GET" path="/api/mailbox/messages" desc="List messages." />
                        <Endpoint method="GET" path="/api/mailbox/messages/:id" desc="Read a full parsed message." />
                        <Endpoint method="DELETE" path="/api/mailbox/messages/:id" desc="Delete a message." />
                        <Endpoint method="WS" path="/api/mailbox/connect?token=" desc="Stream new-mail notifications." />
                    </div>
                    <p>{"Authenticate every request except create with Authorization: Bearer <token>. Keep the token; it is the only credential for the mailbox."}</p>
                </section>

                <section class="content-section">
                    <h2>{"Good to know"}</h2>
                    <div class="info-grid">
                        <div><h3>{"Receive-only"}</h3><p>{"smails receives verification codes, magic links, and confirmations. It cannot send or reply."}</p></div>
                        <div><h3>{"Self-expiring"}</h3><p>{"Inactive mailboxes are wiped automatically. Any request renews an active inbox."}</p></div>
                    </div>
                </section>
            </article>
        </Shell>
    }
}

#[function_component(OtpPage)]
pub fn otp_page() -> Html {
    let base_url = site_base_url();
    let script = format!(
        r#"TOKEN=$(curl -sX POST {base_url}/api/mailbox | jq -r .token)
curl -s {base_url}/api/mailbox/messages \
  -H "Authorization: Bearer $TOKEN" | jq -r '.[0].id'
curl -s {base_url}/api/mailbox/messages/<id> \
    -H "Authorization: Bearer $TOKEN" \
    | jq -r '.text // .html' | grep -oE '\b[0-9]{{6}}\b' | head -1"#
    );

    html! {
        <Shell>
            <article class="content-page">
                <header class="content-hero">
                    <p class="eyebrow">{"OTP - Magic links - Free - No signup"}</p>
                    <h1>{"Read verification codes and magic links from email."}</h1>
                    <p>{"Use the web inbox, CLI, REST API, or MCP server to receive the email and extract the code."}</p>
                    <div class="content-actions">
                        <a class="primary" href="/">{"Get an inbox"}</a>
                        <a class="secondary" href="#how">{"How it works"}</a>
                    </div>
                </header>

                <section id="how" class="content-section">
                    <h2>{"How it works"}</h2>
                    <ol class="step-list">
                        <li>{"Create a disposable mailbox."}</li>
                        <li>{"Use its address wherever a verification code or magic link is required."}</li>
                        <li>{"smails receives the email."}</li>
                        <li>{"Read the message body from the web, CLI, API, or MCP."}</li>
                        <li>{"An agent reads it directly; a script can parse it with regex or JSON tools."}</li>
                    </ol>
                </section>

                <section class="content-section">
                    <h2>{"From the command line"}</h2>
                    <CodeBlock label="terminal" code={"smails create\nsmails inbox\nsmails read <id>"} />
                </section>

                <section class="content-section">
                    <h2>{"In a script"}</h2>
                    <CodeBlock label="bash" code={AttrValue::from(script)} />
                    <p>{"For agents, use the MCP server. For raw automation, use the REST API."}</p>
                </section>

                <section class="content-section">
                    <h2>{"FAQ"}</h2>
                    <details open=true><summary>{"Does smails extract the code automatically?"}</summary><p>{"It returns the full parsed message. Agents can read the code directly; scripts can grep or parse it."}</p></details>
                    <details><summary>{"Is it really free with no signup?"}</summary><p>{"Yes. Create a mailbox with one request and start receiving."}</p></details>
                    <details><summary>{"Will every service deliver to a disposable address?"}</summary><p>{"Most do, but some services block disposable domains."}</p></details>
                </section>
            </article>
        </Shell>
    }
}

fn site_base_url() -> String {
    window()
        .and_then(|window| window.location().origin().ok())
        .filter(|origin| origin != "null")
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned())
}

fn websocket_base_url(base_url: &str) -> String {
    if let Some(rest) = base_url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base_url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        base_url.to_owned()
    }
}

#[derive(Properties, PartialEq)]
struct EndpointProps {
    method: &'static str,
    path: &'static str,
    desc: &'static str,
}

#[function_component(Endpoint)]
fn endpoint(props: &EndpointProps) -> Html {
    html! {
        <div class="endpoint-row">
            <span>{props.method}</span>
            <code>{props.path}</code>
            <p>{props.desc}</p>
        </div>
    }
}
