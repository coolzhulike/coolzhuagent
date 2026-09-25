//! RPR-11a 补强·排空：端到端（本地回环 HTTP）契约测试。
//!
//! 这些用例只用公开 API 驱动真实发送路径（`ProviderClient` / `ApiClient` 的
//! `send_message` / `stream_message`），断言在途登记与排空判定：
//!
//! * 响应头到达但正文仍在生成 → **仍是在途**，消费者不再轮询也不算排空；
//! * 提前 Drop 流 → **远端结果未知**，不是归零；
//! * 超时（本端放弃等待）→ 未知；随后取得可信结束事实 → **迟到事实对账**；
//! * 重复终止回调 → **只结算一次**；
//! * 非流式请求覆盖完整生命周期；请求尚未发出即失败 → 只结清登记；
//! * 身份未知 → **不得**被当作空闲可用。

use std::time::Duration;

use api::{
    drain_reports_permit_automatic_switch, local_endpoint_drain, local_endpoint_drain_all,
    AttemptPhase, AuthSource, DrainVerdict, EndpointIdentity, InputContentBlock, InputMessage,
    MessageRequest, ProviderClient, RequestMode, SettleOutcome,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 本地慢速 SSE 服务端：先发响应头 + `prefix`，停留 `hold` 之后再发 `suffix`。
///
/// `content-length` 覆盖 `prefix + suffix`，所以"响应头已到、正文仍在生成"是可观测状态。
async fn spawn_slow_sse_server(
    prefix: &str,
    suffix: &str,
    hold: Duration,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("local addr");
    let prefix = prefix.to_string();
    let suffix = suffix.to_string();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        read_http_request(&mut socket).await;
        let total = prefix.len() + suffix.len();
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {total}\r\nconnection: close\r\n\r\n"
        );
        socket
            .write_all(head.as_bytes())
            .await
            .expect("head write");
        socket
            .write_all(prefix.as_bytes())
            .await
            .expect("prefix write");
        socket.flush().await.expect("flush");
        tokio::time::sleep(hold).await;
        // 客户端可能已经 Drop 掉连接：写失败不算测试失败。
        let _ = socket.write_all(suffix.as_bytes()).await;
        let _ = socket.flush().await;
    });
    (format!("http://{address}/v1"), handle)
}

/// OpenAI 兼容 SSE：头部一帧（正文生成中）、尾部含 `finish_reason` + `[DONE]`（协议结束事实）。
const OPENAI_SSE_PREFIX: &str = concat!(
    "data: {\"id\":\"chunk-1\",\"model\":\"qwen2.5-vl-3b\",\"choices\":[{\"delta\":{\"content\":\"He\"},\"finish_reason\":null}]}\n\n"
);
const OPENAI_SSE_SUFFIX: &str = concat!(
    "data: {\"id\":\"chunk-1\",\"model\":\"qwen2.5-vl-3b\",\"choices\":[{\"delta\":{\"content\":\"llo\"},\"finish_reason\":null}]}\n\n",
    "data: {\"id\":\"chunk-1\",\"model\":\"qwen2.5-vl-3b\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n"
);

/// Anthropic Messages SSE：协议结束事实是 `message_stop`。
const CLAW_SSE_PREFIX: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_slow\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-6\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":8,\"output_tokens\":0}}}\n\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n"
);
const CLAW_SSE_SUFFIX: &str = concat!(
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"input_tokens\":8,\"output_tokens\":2}}\n\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n\n"
);

fn sample_request(stream: bool) -> MessageRequest {
    MessageRequest {
        model: "qwen2.5-vl-3b".to_string(),
        max_tokens: 64,
        messages: vec![InputMessage::user_text("请慢慢生成")],
        system: None,
        tools: None,
        tool_choice: None,
        reasoning_effort: None,
        stream,
    }
}

fn openai_client(base_url: &str) -> ProviderClient {
    ProviderClient::from_custom_openai_compatible("qwen2.5-vl-3b", base_url, None)
        .expect("custom OpenAI-compatible client should build")
}

/// 裁决规则行「收到响应头但正文仍在生成 → **仍是在途**」+「消费者不再轮询但流仍持有 → 不算排空」。
#[tokio::test]
async fn response_headers_do_not_settle_in_flight_until_protocol_end() {
    let (base_url, server) = spawn_slow_sse_server(
        OPENAI_SSE_PREFIX,
        OPENAI_SSE_SUFFIX,
        Duration::from_millis(600),
    )
    .await;
    let client = openai_client(&base_url);
    let identity = client.endpoint_identity();
    assert!(identity.is_known(), "已解析配置必须给出可切换判定的身份");
    assert_eq!(identity.source(), api::EndpointIdentitySource::ResolvedConfig);

    let mut stream = client
        .stream_message(&sample_request(true))
        .await
        .expect("stream should start after headers");

    // 首帧已到：响应头已收到、正文仍在生成。
    let first = stream.next_event().await.expect("first event");
    assert!(first.is_some());

    // 消费者此刻不再轮询（正在做别的判断）：仍必须算在途。
    let report = local_endpoint_drain(&identity, Duration::from_millis(80)).await;
    assert_eq!(report.verdict, DrainVerdict::InFlightPending);
    assert!(!report.client_drained());
    assert!(!report.permits_automatic_switch());
    assert_eq!(report.status.in_flight, 1);
    assert_eq!(
        report.status.in_flight_attempts[0].phase,
        AttemptPhase::Dispatched
    );
    assert_eq!(
        report.status.in_flight_attempts[0].mode,
        RequestMode::Streaming
    );

    // 读完整个流（拿到协议级完整结束事实）：有界排空能等到结清。
    while stream
        .next_event()
        .await
        .expect("stream event should parse")
        .is_some()
    {}
    let report = local_endpoint_drain(&identity, Duration::from_millis(2000)).await;
    assert_eq!(report.verdict, DrainVerdict::ClientSettled);
    assert!(report.client_drained());
    assert_eq!(report.remote_result_unknown(), 0);
    assert!(report.permits_automatic_switch());
    assert_eq!(stream.request_id(), None);

    drop(stream);
    server.await.expect("server task should finish");
}

/// Anthropic Messages 路径（`claw_provider::MessageStream`）同样把 guard 转移到流对象。
#[tokio::test]
async fn claw_stream_keeps_guard_across_handshake_and_settles_on_message_stop() {
    let (base_url, server) = spawn_slow_sse_server(
        CLAW_SSE_PREFIX,
        CLAW_SSE_SUFFIX,
        Duration::from_millis(400),
    )
    .await;
    let client = api::ApiClient::from_auth(AuthSource::None).with_base_url(base_url);
    let identity = client.endpoint_identity();
    assert!(identity.is_known());

    let mut request = sample_request(true);
    request.model = "claude-sonnet-4-6".to_string();
    let mut stream = client.stream_message(&request).await.expect("stream starts");
    assert!(stream.next_event().await.expect("first event").is_some());

    let report = local_endpoint_drain(&identity, Duration::from_millis(60)).await;
    assert_eq!(report.verdict, DrainVerdict::InFlightPending);
    assert_eq!(report.status.in_flight, 1);

    while stream
        .next_event()
        .await
        .expect("stream event should parse")
        .is_some()
    {}
    let report = local_endpoint_drain(&identity, Duration::from_millis(2000)).await;
    assert_eq!(report.verdict, DrainVerdict::ClientSettled);
    assert_eq!(report.remote_result_unknown(), 0);

    drop(stream);
    server.await.expect("server task should finish");
}

/// 裁决规则行「断流、超时、取消后丢弃流 → 进入远端结果未知，不能直接归零」+「Drop 不能证明服务端已停止计算」。
#[tokio::test]
async fn dropped_stream_becomes_remote_result_unknown_not_zero() {
    let (base_url, server) = spawn_slow_sse_server(
        OPENAI_SSE_PREFIX,
        OPENAI_SSE_SUFFIX,
        Duration::from_millis(1500),
    )
    .await;
    let client = openai_client(&base_url);
    let identity = client.endpoint_identity();

    let mut stream = client.stream_message(&sample_request(true)).await.expect("stream starts");
    assert!(stream.next_event().await.expect("first event").is_some());
    // 消费者提前丢弃流：没有拿到协议级结束事实。
    drop(stream);

    let report = local_endpoint_drain(&identity, Duration::from_millis(50)).await;
    assert!(
        report.client_drained(),
        "本端确实不再持有在途登记（客户端侧已排空）"
    );
    assert_eq!(
        report.remote_result_unknown(),
        1,
        "提前 Drop 只能结束本地持有，不能把远端结果归零"
    );
    assert_eq!(
        report.verdict,
        DrainVerdict::ClientSettledWithRemoteResultUnknown
    );
    assert!(
        !report.permits_automatic_switch(),
        "存在远端结果未知时不得自动切换"
    );

    server.await.expect("server task should finish");
}

/// 裁决规则行「请求已超时、随后取得可信完成事实 → 通过**迟到事实**对账，不重新执行任务」，
/// 以及「收到重复终止回调 → 只能结算一次」。
#[tokio::test]
async fn timeout_marks_unknown_then_late_fact_reconciles_without_replay() {
    let (base_url, server) = spawn_slow_sse_server(
        OPENAI_SSE_PREFIX,
        OPENAI_SSE_SUFFIX,
        Duration::from_millis(400),
    )
    .await;
    let client = openai_client(&base_url);
    let identity = client.endpoint_identity();

    let mut stream = client.stream_message(&sample_request(true)).await.expect("stream starts");
    assert!(stream.next_event().await.expect("first event").is_some());

    // 本端放弃等待（超时/取消），但仍持有流。
    assert_eq!(
        stream.mark_remote_result_unknown(),
        SettleOutcome::Settled,
        "超时 → 远端结果未知"
    );
    assert_eq!(
        stream.mark_remote_result_unknown(),
        SettleOutcome::AlreadySettled,
        "重复终止回调只结算一次"
    );
    let report = local_endpoint_drain(&identity, Duration::from_millis(50)).await;
    assert_eq!(
        report.verdict,
        DrainVerdict::ClientSettledWithRemoteResultUnknown
    );
    assert_eq!(report.remote_result_unknown(), 1);
    assert!(!report.permits_automatic_switch());

    // 迟到事实：继续读流，拿到协议级完整结束事实。
    while stream
        .next_event()
        .await
        .expect("stream event should parse")
        .is_some()
    {}
    let status = stream.inflight_status();
    assert_eq!(status.phase, AttemptPhase::Settled);
    assert!(status.reconciled_late, "迟到事实只对账");
    assert_eq!(status.settlements, 1, "对账不算第二次结算");
    assert_eq!(
        stream.mark_remote_result_unknown(),
        SettleOutcome::AlreadySettled
    );

    let report = local_endpoint_drain(&identity, Duration::from_millis(2000)).await;
    assert_eq!(report.verdict, DrainVerdict::ClientSettled);
    assert_eq!(report.remote_result_unknown(), 0);
    assert!(report.permits_automatic_switch());

    drop(stream);
    server.await.expect("server task should finish");
}

/// 非流式请求覆盖完整请求生命周期：派发后在途，读到完整响应体才结清。
#[tokio::test]
async fn non_streaming_request_covers_full_lifecycle() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        read_http_request(&mut socket).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let body = "{\"id\":\"chatcmpl-slow\",\"model\":\"qwen2.5-vl-3b\",\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"done\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":2}}";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
    });

    let client = openai_client(&format!("http://{address}/v1"));
    let identity = client.endpoint_identity();
    let send_client = client.clone();
    let send = tokio::spawn(async move { send_client.send_message(&sample_request(false)).await });

    tokio::time::sleep(Duration::from_millis(150)).await;
    let report = local_endpoint_drain(&identity, Duration::from_millis(50)).await;
    assert_eq!(
        report.verdict,
        DrainVerdict::InFlightPending,
        "已派发、等待响应体的非流式请求同样是在途"
    );
    assert_eq!(
        report.status.in_flight_attempts[0].mode,
        RequestMode::NonStreaming
    );

    let response = send.await.expect("send task").expect("request succeeds");
    assert_eq!(response.model, "qwen2.5-vl-3b");

    let report = local_endpoint_drain(&identity, Duration::from_millis(2000)).await;
    assert_eq!(report.verdict, DrainVerdict::ClientSettled);
    assert!(report.client_drained());
    assert_eq!(report.remote_result_unknown(), 0);
    assert!(report
        .status
        .recent_settled
        .iter()
        .any(|status| status.mode == RequestMode::NonStreaming
            && status.terminal_fact == Some(api::TerminationFact::ProtocolCompletion)
            && status.settlements == 1));

    server.await.expect("server task");
}

/// 裁决规则行「请求尚未发出即失败或取消 → 可以结清该请求登记」（不产生远端未知）。
#[tokio::test]
async fn request_failing_before_dispatch_settles_registration_cleanly() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    let client = api::ApiClient::from_auth(AuthSource::None).with_base_url(format!("http://{address}"));
    let identity = client.endpoint_identity();

    // 无效图片源在**发送之前**就被校验挡住（本地协议约束）。
    let request = MessageRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 64,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![
                InputContentBlock::Text {
                    text: "看图".to_string(),
                },
                InputContentBlock::ImageUrl {
                    url: "file:///C:/private.png".to_string(),
                    detail: None,
                },
            ],
        }],
        system: None,
        tools: None,
        tool_choice: None,
        reasoning_effort: None,
        stream: false,
    };
    let error = client
        .send_message(&request)
        .await
        .expect_err("无效图片源应在发送前失败");
    assert!(matches!(error, api::ApiError::ConfigError { .. }));

    let report = local_endpoint_drain(&identity, Duration::from_millis(50)).await;
    assert_eq!(
        report.verdict,
        DrainVerdict::ClientSettled,
        "从未发出的请求可以结清登记"
    );
    assert_eq!(report.remote_result_unknown(), 0);
    assert!(report.permits_automatic_switch());
}

/// 裁决约束 3：身份未知/不支持 → 整机闸门拒绝自动切换，且不得被当作"没有在途请求"。
#[tokio::test]
async fn unknown_identity_blocks_automatic_switch_even_when_idle() {
    let unknown = EndpointIdentity::unsupported();
    assert!(!unknown.is_known());
    assert_eq!(unknown.key(), None);

    // 空快照也不行：零在途 + 身份未知 ≠ 空闲可用。
    let report = local_endpoint_drain(&unknown, Duration::from_millis(20)).await;
    assert_eq!(report.verdict, DrainVerdict::IdentityUnknown);
    assert!(!report.identity_known());
    assert!(!report.client_drained());
    assert!(!report.permits_automatic_switch());

    // 未识别身份的登记必须被整机排空看见，并据此拒绝自动切换。
    let unidentified = api::InFlightGuard::register(&unknown, RequestMode::Streaming);
    unidentified.mark_dispatched();
    let reports = local_endpoint_drain_all(Duration::from_millis(20)).await;
    let report = reports
        .iter()
        .find(|report| report.status.identity == unknown)
        .expect("未识别身份的登记不能被整机排空忽略");
    assert_eq!(report.verdict, DrainVerdict::IdentityUnknown);
    assert!(report.status.in_flight >= 1);
    assert!(!drain_reports_permit_automatic_switch(&reports));

    unidentified.settle(api::TerminationFact::ProtocolCompletion);
    assert!(!unidentified.status().is_in_flight());
    assert!(
        !local_endpoint_drain(&unknown, Duration::from_millis(20))
            .await
            .permits_automatic_switch(),
        "身份未知即便结清也不得自动切换"
    );
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let (header_end, content_length) = loop {
        let read = socket.read(&mut chunk).await.expect("request read");
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            break (header_end, length);
        }
    };
    let mut body_read = buffer.len().saturating_sub(header_end + 4);
    while body_read < content_length {
        let mut chunk = vec![0_u8; content_length - body_read];
        let read = socket.read(&mut chunk).await.expect("body read");
        if read == 0 {
            return;
        }
        body_read += read;
    }
}
