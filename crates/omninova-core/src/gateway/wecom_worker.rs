//! WeCom async worker for background processing.

use crate::channels::ChannelKind;
use crate::channels::InboundMessage;
use crate::gateway::wecom_stream::{short_hash, WecomOutboundMsg};
use crate::gateway::GatewayRuntime;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;

const RUNTIME_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub struct WecomAsyncJob {
    pub channel: ChannelKind,
    pub inbound: InboundMessage,
    pub req_id: String,
    pub created_at: u64,
    pub job_id: String,
    pub logical_id: String,
    pub chat_type: String,
    /// Owner generation of the WeCom stream lifecycle that enqueued this
    /// job. A stale generation (Gateway stopped / restarted) must not be
    /// allowed to dispatch a reply.
    pub gen: u64,
}

impl WecomAsyncJob {
    pub fn new(inbound: InboundMessage, req_id: String) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let job_id = format!("wecom_job_{}_{}", created_at, &uuid::Uuid::new_v4().to_string()[..8]);

        Self {
            channel: ChannelKind::Wecom,
            inbound,
            req_id,
            created_at,
            job_id,
            logical_id: "unknown".to_string(),
            chat_type: "unknown".to_string(),
            gen: 0,
        }
    }

    pub fn new_with_writer(
        inbound: InboundMessage,
        req_id: String,
        logical_id: String,
        chat_type: String,
        gen: u64,
    ) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let job_id = format!("wecom_job_{}_{}", created_at, &uuid::Uuid::new_v4().to_string()[..8]);

        Self {
            channel: ChannelKind::Wecom,
            inbound,
            req_id,
            created_at,
            job_id,
            logical_id,
            chat_type,
            gen,
        }
    }
}

pub async fn run_wecom_worker(
    mut receiver: mpsc::Receiver<WecomAsyncJob>,
    runtime: Arc<GatewayRuntime>,
    outbound_tx: mpsc::Sender<WecomOutboundMsg>,
    logical_id: String,
) {
    println!("[wecom-worker] started logical_id={}", logical_id);

    while let Some(job) = receiver.recv().await {
        println!(
            "[wecom-worker] job_received job_id={} logical_id={} chat_type={}",
            job.job_id, job.logical_id, job.chat_type
        );

        let job_runtime = runtime.clone();
        let job_outbound_tx = outbound_tx.clone();

        tokio::spawn(async move {
            process_wecom_job(job, job_runtime, job_outbound_tx).await;
        });
    }

    println!("[wecom-worker] stopped logical_id={}", logical_id);
}

async fn process_wecom_job(
    job: WecomAsyncJob,
    runtime: Arc<GatewayRuntime>,
    outbound_tx: mpsc::Sender<WecomOutboundMsg>,
) {
    let started_at = Instant::now();
    let job_id = &job.job_id;
    let req_id = &job.req_id;
    let msg_id_hash = extract_msg_id(&job.inbound);

    println!(
        "[wecom-worker] agent_dispatch_started job_id={} chat_type={} msg_id={} logical_id={} gen={}",
        job_id, job.chat_type, msg_id_hash, job.logical_id, job.gen
    );

    let runtime_result = timeout(
        Duration::from_secs(RUNTIME_TIMEOUT_SECS),
        runtime.process_inbound(&job.inbound)
    ).await;

    // Generation fencing: if the Gateway stopped / restarted while this job
    // was in flight, the owning stream lifecycle is gone. Discard the reply
    // instead of dispatching to a dead (or a NEW, different) connection.
    if job.gen != 0 && !runtime.is_wecom_stream_generation_active(job.gen) {
        println!(
            "[wecom-worker] agent_dispatch_discarded job_id={} reason=stale_generation gen={} current_gen={}",
            job_id, job.gen, runtime.current_wecom_stream_generation()
        );
        return;
    }

    let reply_text = match runtime_result {
        Ok(Ok(response)) => {
            let duration_ms = started_at.elapsed().as_millis();
            println!(
                "[wecom-worker] agent_dispatch_completed job_id={} reply_len={} duration_ms={}",
                job_id, response.reply.len(), duration_ms
            );
            response.reply
        }
        Ok(Err(e)) => {
            let duration_ms = started_at.elapsed().as_millis();
            println!(
                "[wecom-worker] agent_dispatch_failed job_id={} error={} duration_ms={}",
                job_id, e, duration_ms
            );
            "OmniNova 当前无法完成该请求，请稍后重试。".to_string()
        }
        Err(_) => {
            let duration_ms = started_at.elapsed().as_millis();
            println!(
                "[wecom-worker] agent_dispatch_timeout job_id={} duration_ms={}",
                job_id, duration_ms
            );
            "请求处理超时，请稍后重试。".to_string()
        }
    };

    println!(
        "[wecom-worker] reply_dispatch_requested job_id={} logical_id={}",
        job_id, job.logical_id
    );

    if outbound_tx.send(WecomOutboundMsg::Reply {
        req_id: req_id.clone(),
        text: reply_text,
    }).await.is_err() {
        println!(
            "[wecom-worker] reply_dispatch_failed job_id={} reason=channel_closed",
            job_id
        );
        return;
    }
}

fn extract_msg_id(inbound: &InboundMessage) -> String {
    if let Some(v) = inbound.metadata.get("wecom_msgid") {
        if let Some(s) = v.as_str() {
            return short_hash(s);
        }
    }
    "unknown".to_string()
}
