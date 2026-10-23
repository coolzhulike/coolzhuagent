//! 诊断真实语言服务协议与管理器的结果差异；显式传入工作区和可信二进制。
use std::{collections::BTreeMap,path::PathBuf,time::Duration};
use lsp::{LspManager,LspServerConfig};
use serde_json::json;
mod client {
    include!("../src/client.rs");
    pub async fn raw_definition(client:&LspClient,path:&Path,position:Position)->Result<Value,LspError> {
        let params=json!({"textDocument":{"uri":file_url(path)?},"position":position});
        println!("request={params}");
        client.request::<Value>("textDocument/definition",params).await
    }
}
#[path = "../src/error.rs"]
mod error;
#[path = "../src/types.rs"]
mod types;

#[tokio::main]
async fn main() -> Result<(),Box<dyn std::error::Error>> {
    let args=std::env::args().collect::<Vec<_>>();
    let root=PathBuf::from(&args[1]).canonicalize()?;
    let source=root.join("modules/core-runtime/packages/language-service/src/manager.rs");
    let normalized=lsp::canonical_document_path(&root)?;
    println!("{}",json!({"root":root,"normalized_root":normalized,"document":lsp::canonical_document_path(&source)?,"uri":url::Url::from_file_path(&source).unwrap().to_string()}));
    let config=LspServerConfig {
        name:"rust-analyzer".into(),command:args[2].clone(),args:vec![],env:BTreeMap::from([("CARGO_NET_OFFLINE".into(),"true".into())]),
        workspace_root:root.clone(),extension_to_language:BTreeMap::from([(".rs".into(),"rust".into())]),
        initialization_options:Some(json!({
            "cargo":{"buildScripts":{"enable":false}},"procMacro":{"enable":false},
            "checkOnSave":false,"cachePriming":{"enable":false},"linkedProjects":[root.join("Cargo.toml")],
            "files":{"exclude":(["tmp","dist","package","deploy",".claw",".coolzhu","target"].iter().map(|p|root.join(p)).collect::<Vec<_>>())}
        })),
    };
    let manager=LspManager::new(vec![config.clone()])?;
    manager.sync_document_from_disk(&source).await?;
    let text=std::fs::read_to_string(&source)?;
    let (row,line)=text.lines().enumerate().find(|(_,s)|s.contains("dedupe_locations(&mut locations)")).unwrap();
    let pos=lsp_types::Position::new(row as u32,(line.find("dedupe_locations").unwrap()+2) as u32);
    manager.sync_document_from_disk(&source).await?;
    println!("early={:?}",manager.go_to_definition(&source,pos).await);
    let start=std::time::Instant::now();
    while !manager.is_ready().await && start.elapsed()<Duration::from_secs(180){tokio::time::sleep(Duration::from_secs(1)).await;}
    println!("ready={}; elapsed={:?}",manager.is_ready().await,start.elapsed());
    manager.sync_document_from_disk(&source).await?;
    let result=manager.go_to_definition(&source,pos).await;
    println!("definition={result:?}");
    println!("diagnostics={:?}",manager.collect_workspace_diagnostics().await);
    manager.shutdown().await?;
    // 同一生产客户端独立取原始位置，区分协议转换与管理器边界过滤。
    if result.as_ref().is_ok_and(|x|x.is_empty()) {
        let direct=client::LspClient::connect(types::LspServerConfig {
            name:config.name,command:config.command,args:config.args,env:config.env,
            workspace_root:normalized.clone(),initialization_options:config.initialization_options,
            extension_to_language:config.extension_to_language,
        }).await?;
        let source=lsp::canonical_document_path(&source)?;
        direct.open_document(&source,&text).await?;
        let start=std::time::Instant::now();
        while !direct.is_ready() && start.elapsed()<Duration::from_secs(180){tokio::time::sleep(Duration::from_secs(1)).await;}
        let raw=direct.go_to_definition(&source,pos).await;
        println!("direct_definition={raw:?}");
        println!("raw_definition={:?}",client::raw_definition(&direct,&source,pos).await);
        if let Ok(rows)=raw {for row in rows {
            let canonical=lsp::canonical_document_path(&row.path);
            println!("location={:?}; canonical={canonical:?}; root={normalized:?}; inside={}",row.path,canonical.as_ref().is_ok_and(|p|p.starts_with(&normalized)));
        }}
        direct.shutdown().await?;
    }
    if result?.is_empty(){return Err("真实管理器定义结果为空".into());}
    Ok(())
}
