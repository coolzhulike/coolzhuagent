//! 命令行仅作聊天 HTTP/SSE 客户端；模型、权限、根预算、工具事实均由同一服务负责。
use std::future::Future;
use std::io::{self, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::{Client, Response, Url};
use serde_json::{json, Value};

const MAX_EVENT_BYTES: usize = 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(15);
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);
const HELP: &str = "共享聊天（需先启动此工程的桌面服务）\n\
  coolzhu-cli chat --session ID --room ID --prompt TEXT [--output-format text|json]\n\
  coolzhu-cli chat list           列出此工程真实模型会话与聊天室 ID\n\
  coolzhu-cli chat bind --session ID --room ID   绑定默认 prompt/REPL 目标\n\
  --server http://127.0.0.1:8765   仅接受本机回环 IP，不接受代理或重定向\n\
  --workspace PATH               必须与服务当前工程一致，默认当前目录\n\
  --output-format json           每行一个 SSE 事件，实时输出，不聚合事件历史\n\
会话与聊天室 ID 可在桌面设置/聊天信息中查看；不会猜选当前会话，不改权限。\n\
Ctrl+C 请求停止原轮次并等待确认；断流和提交结果不明时不会自动重试。\n\
默认 prompt/REPL 使用本地绑定；无绑定时拒绝猜选会话。\n\
--legacy-runtime 显式保留旧 .claw 引擎；其配置和权限不会自动导入桌面会话。";

#[derive(Debug)]
struct Options {
    base: Url,
    workspace: PathBuf,
    session: String,
    room: String,
    prompt: String,
    json: bool,
    expected_workspace_id: Option<String>,
}

fn problem(message: impl std::fmt::Display) -> io::Error {
    io::Error::other(message.to_string())
}

fn local_base(value: &str) -> io::Result<Url> {
    let url = Url::parse(value).map_err(|_| problem("服务地址无效"))?;
    let local = url.host_str().and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .is_some_and(|ip| ip.is_loopback());
    if !matches!(url.scheme(), "http" | "https") || !local
        || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some() || url.path() != "/"
    {
        return Err(problem("共享聊天服务必须是无凭据、无路径的本机回环 IP 地址"));
    }
    Ok(url)
}

fn parse(args: &[String]) -> io::Result<Options> {
    let mut base = local_base("http://127.0.0.1:8765")?;
    let mut workspace = std::env::current_dir()?;
    let (mut session, mut room, mut prompt) = (None, None, None);
    let mut json = false;
    let mut seen = std::collections::HashSet::new();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        if !seen.insert(flag) { return Err(problem(format!("参数重复：{flag}"))); }
        if !matches!(flag.as_str(), "--server" | "--workspace" | "--session" | "--room" | "--prompt" | "--output-format") {
            return Err(problem(format!("共享聊天不支持参数 {flag}；会话和权限由桌面服务配置")));
        }
        let value = iter.next().ok_or_else(|| problem(format!("{flag} 缺少值")))?;
        match flag.as_str() {
            "--server" => base = local_base(value)?,
            "--workspace" => workspace = PathBuf::from(value),
            "--session" => session = Some(value.trim().to_string()),
            "--room" => room = Some(value.trim().to_string()),
            "--prompt" => prompt = Some(value.clone()),
            "--output-format" => match value.as_str() {
                "text" => json = false, "json" => json = true,
                _ => return Err(problem("输出格式只能为 text 或 json")),
            },
            _ => unreachable!(),
        }
    }
    let required = |value: Option<String>, label: &str| {
        value.filter(|value| !value.trim().is_empty()).ok_or_else(|| problem(format!("必须显式提供 {label}")))
    };
    let session = required(session, "--session")?;
    let room = required(room, "--room")?;
    if session.len() > 512 || room.len() > 512 { return Err(problem("会话标识过长")); }
    let prompt = required(prompt, "--prompt")?;
    if prompt.len() > MAX_EVENT_BYTES { return Err(problem("输入超过 1 MiB，请改用附件")); }
    Ok(Options { base, workspace: workspace.canonicalize()?, session, room, prompt, json, expected_workspace_id:None })
}

pub(crate) fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args.iter().any(|arg| matches!(arg.as_str(), "--help" | "-h")) {
        println!("{HELP}"); return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "bind") { return bind(&args[1..]); }
    if args.first().is_some_and(|arg| arg == "list") { return list(&args[1..]); }
    let options = parse(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run_async(options, tokio::signal::ctrl_c(), &mut io::stdout(), &mut io::stderr()))?;
    Ok(())
}

pub(crate) fn reject_local_overrides(args: &[String]) -> io::Result<()> {
    if args.iter().any(|arg| ["--model", "--permission-mode", "--allowed-tools", "--allowedTools"]
        .iter().any(|flag| arg == flag || arg.starts_with(&format!("{flag}=")))) {
        return Err(problem("共享聊天使用桌面会话的模型与权限，不能用本地 model/permission/allowed-tools 覆盖；请在设置中更改，或显式选择 --legacy-runtime"));
    }
    Ok(())
}

fn bound_options(prompt: &str, json: bool) -> io::Result<Options> {
    if prompt.trim().is_empty() || prompt.len() > MAX_EVENT_BYTES { return Err(problem("输入为空或超过 1 MiB")); }
    let workspace=std::env::current_dir()?.canonicalize()?;
    let binding=crate::shared_chat_binding::read(&workspace)?.ok_or_else(|| problem(
        "当前工程未绑定共享聊天目标。请先启动此工程桌面服务，运行 coolzhu-cli chat list，再执行 coolzhu-cli chat bind --session ID --room ID。旧脚本可显式加 --legacy-runtime，原 .claw 配置不会被修改。"))?;
    if !same_workspace(&binding.workspace.to_string_lossy(), &workspace)? { return Err(problem("CLI 绑定属于另一工程，请重新 chat bind")); }
    Ok(Options { base:local_base(&binding.server)?, workspace, session:binding.session_id,
        room:binding.room_id, prompt:prompt.to_string(), json, expected_workspace_id:Some(binding.workspace_id) })
}

pub(crate) fn run_bound_prompt(prompt: &str, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let options=bound_options(prompt,json)?;
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(run_async(options,tokio::signal::ctrl_c(),&mut io::stdout(),&mut io::stderr()))?;
    Ok(())
}

pub(crate) fn run_bound_repl() -> Result<(), Box<dyn std::error::Error>> {
    // 进入交互前确认绑定，避免输入了一整轮才发现根本没有目标。
    let options=bound_options("绑定校验",false)?;
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(validate_context(&local_client()?, &options))?;
    println!("共享聊天：模型会话 {} / 聊天室 {}。/exit 退出；/status 查看绑定；Ctrl+C 停止当前轮次。",options.session,options.room);
    let mut editor=rustyline::DefaultEditor::new()?;
    loop {
        let line=match editor.readline("coolzhu> ") {
            Ok(line)=>line,
            Err(rustyline::error::ReadlineError::Interrupted)=>continue,
            Err(rustyline::error::ReadlineError::Eof)=>break,
            Err(error)=>return Err(Box::new(error)),
        };
        let text=line.trim();
        if text.is_empty() {continue;}
        if matches!(text,"/exit"|"/quit") {break;}
        if matches!(text,"/help"|"/status") {
            let current=bound_options("绑定校验",false)?;
            runtime.block_on(validate_context(&local_client()?,&current))?;
            println!("工程：{}\n会话：{}\n聊天室：{}\n模型/权限由桌面设置管理，chat bind 可重新绑定。",current.workspace.display(),current.session,current.room);
            continue;
        }
        if text.starts_with('/') { eprintln!("共享模式仅支持 /help、/status、/exit；其他操作请在桌面设置完成。");continue; }
        let _=editor.add_history_entry(text);
        let options=bound_options(text,false)?;
        if let Err(error)=runtime.block_on(run_async(options,tokio::signal::ctrl_c(),&mut io::stdout(),&mut io::stderr())) {
            eprintln!("{error}");
        }
    }
    Ok(())
}

fn local_client() -> io::Result<Client> {
    Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONTROL_TIMEOUT).build().map_err(problem)
}

fn bind(args:&[String]) -> Result<(),Box<dyn std::error::Error>> {
    let mut parse_args=args.to_vec();
    parse_args.extend(["--prompt".into(),"只读绑定校验".into()]);
    let options=parse(&parse_args)?;
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let workspace_id=runtime.block_on(validate_context(&local_client()?,&options))?;
    let binding=crate::shared_chat_binding::Binding {server:options.base.to_string(),workspace:options.workspace,
        workspace_id,session_id:options.session,room_id:options.room};
    let path=crate::shared_chat_binding::save(&binding)?;
    println!("已绑定默认聊天目标：{} / {}\n绑定文件：{}\n只保存目标引用；权限仍由桌面服务检查。",binding.session_id,binding.room_id,path.display());
    Ok(())
}

fn list(args:&[String]) -> Result<(),Box<dyn std::error::Error>> {
    let mut parse_args=args.to_vec();
    parse_args.extend(["--session".into(),"未投递".into(),"--room".into(),"未投递".into(),"--prompt".into(),"只读列表".into()]);
    let options=parse(&parse_args)?;
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(async {
        let client=local_client()?;
        let state=get_json(&client,options.base.join("api/state").map_err(problem)?).await?;
        if state["app_name"]!="COOLZHU AGENT" || !same_workspace(required_string(&state,"workspace")?,&options.workspace)? {return Err(problem("服务当前工程不同，未读取其会话列表"));}
        for (path,key,label) in [("api/sessions","sessions","模型会话"),("api/chat/rooms","rooms","聊天室")] {
            let data=get_json(&client,options.base.join(path).map_err(problem)?).await?;
            let items=data[key].as_array().ok_or_else(|| problem("服务返回的列表无效"))?;
            for item in items {
                let brief=json!({"kind":key,"id":item["id"],"name":item["name"],"model":item["model"]});
                if options.json {println!("{brief}");}
                else {println!("{label}\t{}\t{}",plain(item["id"].as_str().unwrap_or("")),plain(item["name"].as_str().unwrap_or("")));}
            }
        }
        Ok::<(),io::Error>(())
    })?;
    Ok(())
}

async fn json_response(mut response: Response) -> io::Result<Value> {
    if !response.status().is_success() {
        return Err(problem(format!("聊天服务返回 HTTP {}；未自动重试", response.status().as_u16())));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(problem)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_EVENT_BYTES {
            return Err(problem("服务响应超过读取上限"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(problem)
}

async fn get_json(client: &Client, url: Url) -> io::Result<Value> {
    json_response(client.get(url).timeout(CONTROL_TIMEOUT).send().await.map_err(problem)?).await
}

fn required_string<'a>(value: &'a Value, key: &str) -> io::Result<&'a str> {
    value.get(key).and_then(Value::as_str).filter(|text| !text.is_empty() && text.len() <= 4096)
        .ok_or_else(|| problem(format!("服务响应缺少有效 {key}")))
}

fn same_workspace(server: &str, requested: &Path) -> io::Result<bool> {
    let server = Path::new(server).canonicalize()?;
    // Windows 的盘符和目录名不区分大小写；先 canonicalize 防止相对路径/连接误匹配。
    #[cfg(windows)]
    { Ok(server.to_string_lossy().eq_ignore_ascii_case(&requested.to_string_lossy())) }
    #[cfg(not(windows))]
    { Ok(server == requested) }
}

async fn validate_context(client: &Client, options: &Options) -> io::Result<String> {
    let state = get_json(client, options.base.join("api/state").map_err(problem)?).await?;
    if state["app_name"] != "COOLZHU AGENT" || !same_workspace(required_string(&state, "workspace")?, &options.workspace)? {
        return Err(problem("服务当前工程与 --workspace/当前目录不一致，未提交聊天"));
    }
    if options.expected_workspace_id.as_deref().is_some_and(|expected| state["workspace_id"] != expected) {
        return Err(problem("服务工程身份与本地绑定不一致，未提交聊天，请重新绑定"));
    }
    for (path, key, selected) in [("api/sessions", "sessions", &options.session), ("api/chat/rooms", "rooms", &options.room)] {
        let list = get_json(client, options.base.join(path).map_err(problem)?).await?;
        if !list[key].as_array().is_some_and(|items| items.iter().any(|item| item["id"].as_str() == Some(selected.as_str()))) {
            return Err(problem(format!("当前工程不存在明确选择的 {key} 标识，未提交聊天")));
        }
    }
    Ok(required_string(&state, "workspace_id")?.to_string())
}

/// 字节流按 SSE 行组装后才解码 UTF-8，中文多字节拆包不会变成替换字符。
/// 单行/事件和累计注释均有界；只保留当前事件，不保存整个会话事件 Vec。
#[derive(Default)]
struct Decoder { line: Vec<u8>, event: String, data: String, event_bytes: usize }
impl Decoder {
    fn feed(&mut self, chunk: &[u8], mut emit: impl FnMut(&str, Value) -> io::Result<()>) -> io::Result<()> {
        for &byte in chunk {
            self.event_bytes += 1;
            if self.event_bytes > MAX_EVENT_BYTES { return Err(problem("SSE 单个事件超过 1 MiB")); }
            if byte != b'\n' { self.line.push(byte); continue; }
            if self.line.last() == Some(&b'\r') { self.line.pop(); }
            let line = std::str::from_utf8(&self.line).map_err(problem)?;
            if line.is_empty() {
                if !self.data.is_empty() {
                    self.data.pop(); // SSE 多行 data 的最后换行不属于数据。
                    let value = serde_json::from_str(&self.data).map_err(problem)?;
                    emit(if self.event.is_empty() { "message" } else { &self.event }, value)?;
                }
                self.data.clear(); self.event.clear(); self.event_bytes = 0;
            } else if let Some((field, value)) = line.split_once(':') {
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => { self.event.clear(); self.event.push_str(value); }
                    "data" => { self.data.push_str(value); self.data.push('\n'); }
                    _ => {}, // 注释、id/retry 不触发重连或重发业务。
                }
            }
            self.line.clear();
        }
        Ok(())
    }
}

#[derive(Default)]
struct Receipt { turn: String, run: String, terminal: Option<String> }

struct Output<'a> {
    json: bool, stdout: &'a mut dyn Write, stderr: &'a mut dyn Write,
    receipt: Receipt, message_id: String, displayed: String,
}
fn plain(value: &str) -> String {
    value.chars().filter(|c| !c.is_control() || matches!(c, '\n' | '\t')).collect()
}
impl Output<'_> {
    fn event(&mut self, event: &str, value: Value, room: &str) -> io::Result<()> {
        if self.receipt.terminal.is_some() { return Err(problem("终态之后收到额外事件")); }
        match event {
            "started" => {
                if !self.receipt.run.is_empty() || value["chat_room_id"] != room { return Err(problem("服务返回了重复或不匹配的轮次")); }
                self.receipt.run = required_string(&value, "run_id")?.to_string();
                self.receipt.turn = required_string(&value, "turn_id")?.to_string();
            },
            "done" => {
                if self.receipt.run.is_empty() || value["run_id"] != self.receipt.run || value["turn_id"] != self.receipt.turn {
                    return Err(problem("终态缺少原轮次的身份，结果待确认"));
                }
                let status = required_string(&value, "status")?;
                if !matches!(status, "completed" | "failed" | "interrupted" | "commit_pending") { return Err(problem("服务返回了未知终态")); }
                self.receipt.terminal = Some(status.to_string());
            },
            _ => {},
        }
        if self.json {
            serde_json::to_writer(&mut self.stdout, &json!({ "event": event, "data": value })).map_err(problem)?;
            writeln!(self.stdout)?; self.stdout.flush()?;
            return Ok(());
        }
        match event {
            "started" => writeln!(self.stderr, "已接纳轮次 {}", plain(&self.receipt.run))?,
            "error" => writeln!(self.stderr, "{}", plain(value["message"].as_str().unwrap_or("聊天服务发生错误")))?,
            "done" => writeln!(self.stderr, "\n轮次状态：{}", self.receipt.terminal.as_deref().unwrap_or("待确认"))?,
            "message" | "message_start" | "message_delta" | "message_replace" => {
                let kind = value["kind"].as_str().unwrap_or("");
                if !kind.starts_with("assistant-") { return Ok(()); }
                let id = required_string(&value, "id")?;
                if self.message_id != id {
                    if !self.message_id.is_empty() { writeln!(self.stdout)?; }
                    self.message_id = id.to_string(); self.displayed.clear();
                }
                let content = plain(value[if event == "message_delta" { "delta" } else { "content" }].as_str().unwrap_or(""));
                if event == "message_delta" {
                    write!(self.stdout, "{content}")?;
                    if self.displayed.len().saturating_add(content.len()) <= MAX_EVENT_BYTES { self.displayed.push_str(&content); }
                    else { return Err(problem("单条回复超过 CLI 显示上限，完整记录请在聊天室查看")); }
                } else if let Some(suffix) = content.strip_prefix(&self.displayed) {
                    write!(self.stdout, "{suffix}")?; self.displayed = content;
                } else if content != self.displayed {
                    writeln!(self.stdout, "\n{content}")?; self.displayed = content;
                }
                self.stdout.flush()?;
            },
            _ => {},
        }
        self.stderr.flush()
    }
}

async fn interrupt(client: &Client, options: &Options, receipt: &Receipt) -> io::Result<Value> {
    json_response(client.post(options.base.join("api/chat/turn/interrupt").map_err(problem)?)
        .timeout(CONTROL_TIMEOUT).json(&json!({"session_id":options.session,"chat_room_id":options.room,"turn_id":receipt.turn}))
        .send().await.map_err(problem)?).await
}

async fn confirm_after_disconnect(client: &Client, options: &Options, workspace_id: &str, output: &mut Output<'_>) -> io::Result<()> {
    if output.receipt.run.is_empty() { return Err(problem("连接结束且未收到接纳标识，提交结果未知；请查看聊天室，勿自动重试")); }
    let deadline = tokio::time::Instant::now() + CLEANUP_TIMEOUT;
    let mut url = options.base.clone();
    url.path_segments_mut().map_err(|()| problem("服务路径不可用"))?.extend(["api", "runs", &output.receipt.run]);
    loop {
        let status = tokio::time::timeout_at(deadline, get_json(client, url.clone())).await
            .map_err(|_| problem("原轮次仍未收尾，结果待确认；不要自动重试"))??;
        if status["run_id"] != output.receipt.run || status["turn_id"] != output.receipt.turn
            || status["session_id"] != options.session || status["chat_room_id"] != options.room
            || status["workspace_id"] != workspace_id
        { return Err(problem("查询返回的运行身份不匹配，结果待确认")); }
        if let Some(state) = status["state"].as_str().filter(|state| matches!(*state, "completed" | "failed" | "interrupted" | "orphaned")) {
            let state = if state == "orphaned" { "failed" } else { state };
            output.event("done", json!({"run_id":output.receipt.run,"turn_id":output.receipt.turn,"status":state,"source":"run_status_after_disconnect"}), &options.room)?;
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline { return Err(problem("原轮次仍未收尾，结果待确认；不要自动重试")); }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn run_async(options: Options, signal: impl Future<Output = io::Result<()>>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> io::Result<()> {
    // 与本地 GUI HTTP API 相同的认证边界：现有服务无另一个客户端 token。
    // 不接受远程地址、不继承系统代理、不跟随重定向，也不发送模型密钥。
    let client = local_client()?;
    let workspace_id = validate_context(&client, &options).await?;
    let mut output = Output { json: options.json, stdout, stderr, receipt: Receipt::default(), message_id: String::new(), displayed: String::new() };
    tokio::pin!(signal);
    let mut cancel_requested = false;
    let send = client.post(options.base.join("api/chat/send/stream").map_err(problem)?)
        .header("Accept", "text/event-stream")
        .json(&json!({"expected_workspace_id":workspace_id,"session_id":options.session,"chat_room_id":options.room,"target_agent_ids":[options.session],"text":options.prompt}))
        .send();
    let send = tokio::time::timeout(IDLE_TIMEOUT, send);
    tokio::pin!(send);
    let response = loop {
        tokio::select! {
            result = &mut signal, if !cancel_requested => { result?; cancel_requested = true; },
            result = &mut send => break result.map_err(|_| problem("提交等待超时，是否接纳未知；请查看聊天室，勿重发"))?.map_err(problem)?,
        }
    };
    if !response.status().is_success() { json_response(response).await?; unreachable!(); }
    if !response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|value| value.to_str().ok()).is_some_and(|value| value.starts_with("text/event-stream")) {
        return Err(problem("服务未返回 SSE；提交结果未知，未自动重试"));
    }
    let mut response = response;
    let mut decoder = Decoder::default();
    let mut cancel_sent = false;
    let mut stop_deadline = None;
    loop {
        if output.receipt.terminal.is_some() { break; }
        if cancel_requested && !cancel_sent && !output.receipt.run.is_empty() {
            // 原会话+聊天室+turn 做停止 CAS；不能仅凭客户端拼一个 run_id 请求停止。
            let confirmation = interrupt(&client, &options, &output.receipt).await?;
            if confirmation["turn_id"] != output.receipt.turn { return Err(problem("停止响应不属于原轮次")); }
            cancel_sent = true; stop_deadline = Some(tokio::time::Instant::now() + CLEANUP_TIMEOUT);
            if options.json {
                serde_json::to_writer(&mut output.stdout, &json!({"event":"interrupt_requested","data":confirmation})).map_err(problem)?;
                writeln!(output.stdout)?; output.stdout.flush()?;
            } else { writeln!(output.stderr, "已请求停止原轮次，等待服务确认收尾…")?; }
        }
        let deadline = stop_deadline.unwrap_or_else(|| tokio::time::Instant::now() + IDLE_TIMEOUT);
        tokio::select! {
            result = &mut signal, if !cancel_requested => { result?; cancel_requested = true; },
            chunk = tokio::time::timeout_at(deadline, response.chunk()) => match chunk {
                Ok(Ok(Some(bytes))) => {
                    if let Err(error) = decoder.feed(&bytes, |event, value| output.event(event, value, &options.room)) {
                        // 协议/输出错误不重放业务；已知原身份时尽力停止，保留原错误归因。
                        if !output.receipt.run.is_empty() { let _ = interrupt(&client, &options, &output.receipt).await; }
                        return Err(error);
                    }
                },
                _ => {
                    if !output.receipt.run.is_empty() && !cancel_sent { interrupt(&client, &options, &output.receipt).await?; }
                    confirm_after_disconnect(&client, &options, &workspace_id, &mut output).await?;
                    break;
                },
            }
        }
    }
    if !options.json { writeln!(output.stdout)?; }
    match output.receipt.terminal.as_deref() {
        Some("completed") => Ok(()),
        Some(state) => Err(problem(format!("原轮次终态：{state}；未自动重试"))),
        None => Err(problem("未收到确认终态；未自动重试")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    #[test]
    fn decoder_handles_split_utf8_and_rejects_oversize_and_wrong_terminal_identity() {
        let data = "event: message_delta\r\ndata: {\"delta\":\"鹈鹕骑车\"}\r\n\r\n";
        let mut decoder = Decoder::default();
        let mut received = None;
        for byte in data.as_bytes() {
            decoder.feed(&[*byte], |event, value| { assert_eq!(event, "message_delta"); received = Some(value); Ok(()) }).unwrap();
        }
        assert_eq!(received.unwrap()["delta"], "鹈鹕骑车");
        assert!(decoder.feed(&vec![b'x'; MAX_EVENT_BYTES + 1], |_, _| Ok(())).is_err());
        assert!(local_base("https://example.org").is_err());
        assert!(local_base("http://127.0.0.1@evil.example").is_err());
        assert!(local_base("http://127.0.0.1/path").is_err());
        assert!(local_base("http://[::1]:8765").is_ok());
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut output = Output {json:true,stdout:&mut out,stderr:&mut err,receipt:Receipt::default(),message_id:String::new(),displayed:String::new()};
        output.event("started", json!({"run_id":"run-one","turn_id":"turn-one","chat_room_id":"room"}), "room").unwrap();
        assert!(output.event("done", json!({"run_id":"other-run","turn_id":"turn-one","status":"completed"}), "room").is_err());
        assert!(output.receipt.terminal.is_none());
    }

    async fn read_request(socket: &mut TcpStream) -> (String, Value) {
        let mut bytes = Vec::new();
        let header_end = loop {
            let mut byte = [0;1]; socket.read_exact(&mut byte).await.unwrap(); bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") { break bytes.len(); }
            assert!(bytes.len() < 64 * 1024);
        };
        let header = String::from_utf8(bytes).unwrap();
        let length = header.lines().find_map(|line| line.to_lowercase().strip_prefix("content-length:").map(|value| value.trim().parse::<usize>().unwrap())).unwrap_or(0);
        assert!(length < MAX_EVENT_BYTES && header_end < MAX_EVENT_BYTES);
        let mut body = vec![0; length]; socket.read_exact(&mut body).await.unwrap();
        (header.lines().next().unwrap().to_string(), if body.is_empty() { Value::Null } else { serde_json::from_slice(&body).unwrap() })
    }
    async fn send_json(socket: &mut TcpStream, value: Value) {
        let body = serde_json::to_vec(&value).unwrap();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    }
    async fn chunk(socket: &mut TcpStream, bytes: &[u8]) {
        socket.write_all(format!("{:x}\r\n",bytes.len()).as_bytes()).await.unwrap();
        socket.write_all(bytes).await.unwrap(); socket.write_all(b"\r\n").await.unwrap();
    }

    /// 使用真实本机 HTTP 连接核验 adapter 发的接口/身份和终态；不运行模型或工具。
    #[tokio::test]
    async fn shared_http_stream_and_cancel_use_one_real_admission_and_original_turn() {
        for cancel in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base = local_base(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            let workspace = std::env::current_dir().unwrap().canonicalize().unwrap();
            let server_workspace = workspace.clone();
            let (notify, notified) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let mut held_stream = None;
                let mut notify = Some(notify);
                let mut submissions = 0;
                for _ in 0..if cancel {5} else {4} {
                    let (mut socket,_) = listener.accept().await.unwrap();
                    let (request, body) = read_request(&mut socket).await;
                    match request.as_str() {
                        "GET /api/state HTTP/1.1" => send_json(&mut socket,json!({"app_name":"COOLZHU AGENT","workspace":server_workspace,"workspace_id":"workspace"})).await,
                        "GET /api/sessions HTTP/1.1" => send_json(&mut socket,json!({"sessions":[{"id":"session"}]})).await,
                        "GET /api/chat/rooms HTTP/1.1" => send_json(&mut socket,json!({"rooms":[{"id":"room"}]})).await,
                        "POST /api/chat/send/stream HTTP/1.1" => {
                            submissions += 1;
                            assert_eq!(body["session_id"],"session"); assert_eq!(body["chat_room_id"],"room");
                            assert_eq!(body["target_agent_ids"],json!(["session"]));
                            assert_eq!(body["expected_workspace_id"],"workspace");
                            assert!(body.get("permission_mode").is_none());
                            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                            chunk(&mut socket,b"event: started\ndata: {\"run_id\":\"run\",\"turn_id\":\"turn\",\"chat_room_id\":\"room\"}\n\n").await;
                            if cancel {
                                notify.take().unwrap().send(()).unwrap(); held_stream = Some(socket);
                            } else {
                                let events = "event: message_delta\ndata: {\"id\":\"answer\",\"kind\":\"assistant-reply\",\"delta\":\"鹈鹕骑车\"}\n\nevent: done\ndata: {\"run_id\":\"run\",\"turn_id\":\"turn\",\"status\":\"completed\"}\n\n";
                                for bytes in events.as_bytes().chunks(2) { chunk(&mut socket,bytes).await; }
                                socket.write_all(b"0\r\n\r\n").await.unwrap();
                            }
                        },
                        "POST /api/chat/turn/interrupt HTTP/1.1" => {
                            assert_eq!(body,json!({"session_id":"session","chat_room_id":"room","turn_id":"turn"}));
                            send_json(&mut socket,json!({"turn_id":"turn","status":"interrupt_requested","outcome":"stop_requested"})).await;
                            let mut stream = held_stream.take().unwrap();
                            chunk(&mut stream,b"event: done\ndata: {\"run_id\":\"run\",\"turn_id\":\"turn\",\"status\":\"interrupted\"}\n\n").await;
                            stream.write_all(b"0\r\n\r\n").await.unwrap();
                        },
                        _ => panic!("意外或重复请求：{request}"),
                    }
                }
                assert_eq!(submissions,1);
            });
            let signal = async move {
                if cancel { notified.await.map_err(problem)?; } else { std::future::pending::<()>().await; }
                Ok(())
            };
            let options = Options {base,workspace,session:"session".into(),room:"room".into(),prompt:"测试".into(),json:true,expected_workspace_id:None};
            let (mut out,mut err)=(Vec::new(),Vec::new());
            let result = tokio::time::timeout(Duration::from_secs(10),run_async(options,signal,&mut out,&mut err)).await.unwrap();
            assert_eq!(result.is_err(),cancel);
            server.await.unwrap();
            let events = String::from_utf8(out).unwrap();
            let last:Value=serde_json::from_str(events.lines().last().unwrap()).unwrap();
            assert_eq!(last["event"],"done"); assert_eq!(last["data"]["status"],if cancel {"interrupted"} else {"completed"});
            if !cancel { assert!(events.contains("鹈鹕骑车")); }
        }
    }
}
