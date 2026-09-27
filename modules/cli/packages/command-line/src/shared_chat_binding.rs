//! 本地绑定只保存真实目标引用；它不携带密钥、权限授予或新的模型配置。
use std::{fs, io::{self, Read, Write}, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use serde_json::{json, Value};

pub(crate) struct Binding {
    pub(crate) server: String,
    pub(crate) workspace: PathBuf,
    pub(crate) workspace_id: String,
    pub(crate) session_id: String,
    pub(crate) room_id: String,
}
fn path(workspace: &Path) -> PathBuf { workspace.join(".coolzhu").join("cli-chat.json") }
pub(crate) fn read(workspace: &Path) -> io::Result<Option<Binding>> {
    let file = match fs::File::open(path(workspace)) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new(); file.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 { return Err(io::Error::other("CLI 绑定文件过大")); }
    let data:Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if data["version"] != 1 { return Err(io::Error::other("CLI 绑定版本不受支持，请重新执行 chat bind")); }
    let get = |field:&str| data[field].as_str().filter(|value| !value.is_empty() && value.len()<=4096)
        .map(str::to_string).ok_or_else(|| io::Error::other(format!("CLI 绑定缺少 {field}")));
    Ok(Some(Binding { server:get("server")?,workspace:PathBuf::from(get("workspace")?),
        workspace_id:get("workspace_id")?,session_id:get("session_id")?,room_id:get("chat_room_id")? }))
}
pub(crate) fn save(binding:&Binding) -> io::Result<PathBuf> {
    let path=path(&binding.workspace);
    let directory=path.parent().ok_or_else(|| io::Error::other("CLI 绑定路径无效"))?;
    fs::create_dir_all(directory)?;
    let unique=SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
    let temporary=directory.join(format!("cli-chat.{}.{unique}.tmp",std::process::id()));
    let result=(|| {
        let mut file=fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        serde_json::to_writer_pretty(&mut file,&json!({"version":1,"server":binding.server,
            "workspace":binding.workspace,"workspace_id":binding.workspace_id,
            "session_id":binding.session_id,"chat_room_id":binding.room_id})).map_err(io::Error::other)?;
        file.write_all(b"\n")?; file.sync_all()?; drop(file);
        fs::rename(&temporary,&path)
    })();
    if result.is_err() { let _=fs::remove_file(&temporary); }
    result.map(|()|path)
}
