//! 诊断文件全局磁盘配额核心：只管理**显式登记**的成员文件集合。
//! 绝不扫描目录、不按扩展名清理、不纳入 tool-audit/computer-use-audit/SQLite/会话历史等权威数据。
//! 未登记的同名目录下文件既不计入也不删除；失败一律返回错误，权威文件不受影响。
use std::{
    collections::{BTreeSet, HashMap},
    sync::{Mutex, OnceLock},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

const QUOTA_LOCK_NAME: &str = ".diag-quota.lock";
const QUOTA_MEMBERS_NAME: &str = ".diag-quota.members";
// 清单属于诊断控制数据，同样必须有界；达到上限后拒绝新登记，不丢弃旧成员账目。
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_MANIFEST_MEMBERS: usize = 4096;
// 仅这三个生产日志族可由另一产品进程显式接管。清单内容不能扩展此权限。
const PRODUCT_LOG_FAMILIES: &[&str] = &[
    "coolzhu-agent.jsonl", "coolzhu-tauri-shell.jsonl", "coolzhu-web-console.jsonl",
];
/// 活跃写入宽限：最近 ACTIVE_WRITE_GRACE 内被修改的成员视为仍在使用，不淘汰。
use std::time::Duration;
const ACTIVE_WRITE_GRACE: Duration = Duration::from_secs(30);

/// 一次 enforce 的如实报告：实际保留/丢弃范围，不把单文件上限冒充全局。
#[derive(Debug)]
pub struct QuotaReport {
    /// 执行前登记成员合计字节
    pub total_before: u64,
    /// 执行后登记成员合计字节
    pub total_after: u64,
    /// 实际被淘汰的成员（路径, 字节）
    pub evicted: Vec<(PathBuf, u64)>,
    /// 登记但未计数的成员（路径, 原因）：缺失/符号链接/重解析点/非普通文件/活跃窗口/祖先链接
    pub skipped: Vec<(PathBuf, &'static str)>,
    /// 尝试淘汰但删除失败的成员（路径, OS错误）：部分进度可核对，已删与未删都如实列出
    pub failed: Vec<(PathBuf, String)>,
}

/// 注册根 + 显式登记成员 + 共享总量预算。成员解析被限制在注册根内。
pub struct DiagnosticQuota {
    root: PathBuf,
    root_canonical: PathBuf,
    root_id: (u64, u64),
    budget_bytes: u64,
    members: BTreeSet<PathBuf>,
}

impl DiagnosticQuota {
    /// root 必须是已存在目录；budget_bytes 必须 >0 且不超过 4GiB（与 bundle 硬上限同量级）。
    pub fn new(root: impl Into<PathBuf>, budget_bytes: u64) -> io::Result<Self> {
        let root = root.into();
        if budget_bytes == 0 || budget_bytes > 4 * 1024 * 1024 * 1024 {
            return Err(invalid("预算必须为正且不超过 4GiB"));
        }
        let meta = fs::metadata(&root)
            .map_err(|e| io::Error::new(e.kind(), format!("注册根不可用: {}", root.display())))?;
        if !meta.is_dir() {
            return Err(invalid("注册根不是目录"));
        }
        if fs::symlink_metadata(&root).map(|m| m.file_type().is_symlink()).unwrap_or(true) {
            return Err(invalid("注册根为符号链接/重解析点，拒绝（fail-closed）"));
        }
        let root_canonical = fs::canonicalize(&root)
            .map_err(|e| io::Error::new(e.kind(), "注册根规范化失败"))?;
        let root_id = win::open_dir(&root).ok().and_then(|h| win::info(&h))
            .map(|i| (i.volume, i.index))
            .ok_or_else(|| invalid("注册根身份不可验证，拒绝"))?;
        Ok(Self { root, root_canonical, root_id, budget_bytes, members: BTreeSet::new() })
    }

    /// 显式登记一个成员文件（注册根内相对路径）。绝对路径/父级逃逸/前缀组件拒绝。
    pub fn register_file(&mut self, relative: &str) -> io::Result<()> {
        let rel = validate_member(relative)?;
        self.persist_members(std::slice::from_ref(&rel))?;
        self.members.insert(rel);
        Ok(())
    }

    /// 登记轮转族：base 及 base.1..=generations 全部成员（与 rolling_file 命名一致）。
    pub fn register_rotated(&mut self, relative: &str, generations: usize) -> io::Result<()> {
        let rel = validate_member(relative)?;
        if generations == 0 || generations > 16 {
            return Err(invalid("轮转代数必须在 1..=16"));
        }
        let mut names = vec![rel];
        for g in 1..=generations {
            let mut name = names[0].as_os_str().to_os_string();
            name.push(format!(".{g}"));
            names.push(PathBuf::from(name));
        }
        self.persist_members(&names)?;
        for name in names { self.members.insert(name); }
        Ok(())
    }

    /// 登记成员当前合计字节（缺失/链接成员计 0）。
    pub fn accounted_bytes(&self) -> io::Result<u64> {
        let mut total = 0_u64;
        for member in &self.effective_members()? {
            total = total.saturating_add(self.member_size(member)?);
        }
        Ok(total)
    }

    /// 拒写策略用：再写 additional 字节是否越过预算。只读判定，不改状态。
    pub fn would_exceed(&self, additional: u64) -> io::Result<bool> {
        Ok(self.accounted_bytes()?.saturating_add(additional) > self.budget_bytes)
    }

    /// 配额锁文件路径（注册根内、固定名，不计入成员集合）。
    pub fn lock_file_path(&self) -> PathBuf { self.root.join(QUOTA_LOCK_NAME) }

    /// 淘汰最旧登记成员直到总量 <= 预算。持有跨进程锁；锁忙立即报错（fail-closed）。
    /// 只删除：显式登记 + 各层父目录均非链接/重解析 + 自身普通文件 + 非硬链接 +
    /// mtime 活跃窗口之外的成员。注意：mtime 窗口只是启发式，不能证明无人持有句柄；
    /// 真实持柄由删除失败路径兜底——记入 failed 并继续。
    /// 删除失败记入 failed 并继续，已删事实如实出现在 evicted，绝不丢失部分进度。
    /// 残余限制：元数据校验与 remove_file 之间仍存在极窄替换窗口；Windows 上
    /// remove_file 删除重解析点本身而非目标，因此不会误删根外内容。
    pub fn enforce(&self) -> io::Result<QuotaReport> {
        self.enforce_with_headroom(0)
    }

    /// 与 enforce 相同，但为即将写入的 headroom 字节预留空间：
    /// 淘汰到登记成员总量 <= 预算-headroom 为止，供共享写路径原子接续 append_gated。
    pub(crate) fn enforce_with_headroom(&self, headroom: u64) -> io::Result<QuotaReport> {
        let lock_path = self.lock_file_path();
        self.verify_root()?;
        let lock = OpenOptions::new().create(true).write(true).open(&lock_path)?;
        lock.try_lock().map_err(|e| io::Error::other(format!("配额锁忙或不可用: {e}")))?;
        // 锁句柄持有到函数结束。
        let now = SystemTime::now();
        let mut evicted = Vec::new();
        let mut skipped = Vec::new();
        let mut failed = Vec::new();
        let mut live: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
        let mut total: u64 = 0;
        for member in &self.effective_members()? {
            let path = self.root.join(member);
            // 祖先目录逐层验证：任一为链接/重解析/缺失即不碰该成员（嵌套链接可指向根外）。
            if !self.ancestors_clean(member) {
                skipped.push((path, "ancestor-unverified"));
                continue;
            }
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    skipped.push((path, "missing"));
                }
                Err(e) => return Err(e),
                Ok(meta) if meta.file_type().is_symlink() => {
                    skipped.push((path, "symlink-or-reparse"));
                }
                Ok(meta) if !meta.is_file() => {
                    skipped.push((path, "not-regular-file"));
                }
                Ok(meta) => {
                    // 句柄级核查：硬链接别名或真实位置出根都拒绝管理；仅查询不写不删。
                    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    let size = meta.len();
                    match File::open(&path).ok().and_then(|f| win::info(&f)).zip(
                        fs::canonicalize(&path).ok()) {
                        Some((info, _)) if info.links > 1 => {
                            total = total.saturating_add(size);
                            skipped.push((path, "hardlink-alias"));
                        }
                        Some((info, real))
                            if !path_within(&real, &self.root_canonical)
                                || info.attrs & win::FILE_ATTRIBUTE_REPARSE_POINT != 0 => {
                            total = total.saturating_add(size);
                            skipped.push((path, "outside-root"));
                        }
                        Some(_)
                            if now.duration_since(mtime).map(|a| a < ACTIVE_WRITE_GRACE).unwrap_or(true) => {
                            // 活跃窗口是启发式而非持柄证明；成员仍计入真实占用，不参与本轮淘汰。
                            total = total.saturating_add(size);
                            skipped.push((path, "active-window"));
                        }
                        Some(_) => {
                            total = total.saturating_add(size);
                            if self.members.contains(member) {
                                live.push((path, size, mtime));
                            } else {
                                // 仅清单登记的成员计入真实占用但永不被本进程淘汰，
                                // 损坏/伪造清单不能把权威文件变成可淘汰对象。
                                skipped.push((path, "foreign-registration"));
                            }
                        }
                        None => {
                            total = total.saturating_add(size);
                            skipped.push((path, "identity-unverified"));
                        }
                    }
                }
            }
        }
        let total_before = total;
        // 淘汰顺序：修改时间最早者优先（最旧代/最久未写先出）。
        live.sort_by_key(|(_, _, mtime)| *mtime);
        for (path, size, _) in live {
            if total <= self.budget_bytes.saturating_sub(headroom) { break; }
            // 删除前再核一遍：祖先/自身身份在扫描后可能被替换（收窄残余窗口）。
            let rel = match path.strip_prefix(&self.root) {
                Ok(rel) => rel,
                Err(_) => { failed.push((path, "outside-root".into())); continue; }
            };
            let still_clean = self.ancestors_clean(rel)
                && fs::symlink_metadata(&path)
                    .map(|m| m.is_file() && !m.file_type().is_symlink())
                    .unwrap_or(false);
            if !still_clean {
                failed.push((path, "identity-changed".into()));
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => {
                    total = total.saturating_sub(size);
                    evicted.push((path, size));
                }
                Err(e) => failed.push((path, e.to_string())),
            }
        }
        drop(lock);
        Ok(QuotaReport { total_before, total_after: total, evicted, skipped, failed })
    }

    /// 受同一配额锁保护的 gated 追加：检查+写入原子化，使用本接口的写者不会共同超预算。
    /// 锁忙立即失败（fail-closed，不重试阻塞业务）；未登记成员/越预算/祖先不洁净一律拒绝。
    /// 绕过本接口的直接写入不在此保证内——生产接线必须只经此路径写入。
    pub fn append_gated(&self, member: &str, record: &[u8]) -> io::Result<()> {
        if record.is_empty() { return Ok(()); }
        let rel = validate_member(member)?;
        if !self.members.contains(&rel) {
            return Err(invalid("成员未登记，拒绝写入"));
        }
        let lock_path = self.lock_file_path();
        self.verify_root()?;
        let lock = OpenOptions::new().create(true).write(true).open(&lock_path)?;
        lock.try_lock().map_err(|e| io::Error::other(format!("配额锁忙，拒绝写入: {e}")))?;
        let used = self.accounted_bytes()?;
        if used.saturating_add(record.len() as u64) > self.budget_bytes {
            return Err(invalid("写入将超出配额预算，拒绝"));
        }
        let mut file = self.open_append_verified(&rel)?;
        file.write_all(record)
    }

    /// 共享登记清单路径（注册根内固定名，本身永远不是成员、不计数不淘汰）。
    fn manifest_path(&self) -> PathBuf { self.root.join(QUOTA_MEMBERS_NAME) }

    /// 配额锁内仅追加未登记成员，重复写日志不会再次增长清单。
    /// 清单是跨进程合作写者共享总额的唯一持久依据；写入失败即拒绝登记，不写本地状态。
    fn persist_members(&self, names: &[PathBuf]) -> io::Result<()> {
        if names.is_empty() { return Ok(()); }
        self.verify_root()?;
        let lock = OpenOptions::new().create(true).write(true).open(self.lock_file_path())?;
        lock.try_lock().map_err(|e| io::Error::other(format!("配额锁忙，登记失败: {e}")))?;
        let mut known = self.persisted_members()?;
        let mut addition = String::new();
        for name in names {
            if known.insert(name.clone()) {
                addition.push_str(&name.to_string_lossy().replace('\\', "/"));
                addition.push('\n');
            }
        }
        if known.len() > MAX_MANIFEST_MEMBERS { return Err(invalid("诊断成员数量超过上限")); }
        if addition.is_empty() { return Ok(()); }
        // 句柄级核验复用成员追加的同一身份检查：硬链接别名/重解析点/根外实体一律拒绝。
        let mut file = self.open_append_verified(Path::new(QUOTA_MEMBERS_NAME))?;
        if file.metadata()?.len().saturating_add(addition.len() as u64) > MAX_MANIFEST_BYTES {
            return Err(invalid("诊断成员清单超过字节上限"));
        }
        file.write_all(addition.as_bytes())?;
        Ok(())
    }

    /// 读共享清单：逐行过成员校验，非法/越界行一律跳过（绝不管未登记文件）；
    /// 清单缺失视为无跨进程登记，读取错误 fail-closed。
    fn persisted_members(&self) -> io::Result<BTreeSet<PathBuf>> {
        let mut out = BTreeSet::new();
        let mpath = self.manifest_path();
        // 打开句柄即被校验对象：重解析点/符号链接、硬链接别名、规范路径出根、
        // 句柄与规范身份不一致，任一命中即 fail-closed——不读不写。
        let file = match File::open(&mpath) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e),
            Ok(f) => f,
        };
        let info = win::info(&file).ok_or_else(|| invalid("清单身份不可验证"))?;
        if info.attrs & win::FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(invalid("清单为重解析点/符号链接，拒绝"));
        }
        if info.links > 1 { return Err(invalid("清单存在硬链接别名，拒绝")); }
        let canonical = fs::canonicalize(&mpath)
            .map_err(|e| io::Error::new(e.kind(), "清单规范化失败"))?;
        if !path_within(&canonical, &self.root_canonical) {
            return Err(invalid("清单真实位置在注册根外，拒绝"));
        }
        let cfile = File::open(&canonical)
            .map_err(|e| io::Error::new(e.kind(), "清单规范路径无法打开验证句柄"))?;
        let cinfo = win::info(&cfile).ok_or_else(|| invalid("清单规范身份不可验证"))?;
        if (cinfo.volume, cinfo.index) != (info.volume, info.index) {
            return Err(invalid("清单句柄与规范路径身份不一致，拒绝"));
        }
        if file.metadata()?.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("诊断成员清单超过字节上限"));
        }
        let mut text = String::new();
        file.take(MAX_MANIFEST_BYTES + 1).read_to_string(&mut text)?;
        if text.len() as u64 > MAX_MANIFEST_BYTES { return Err(invalid("诊断成员清单读取超过上限")); }
        for line in text.lines() {
            if let Ok(rel) = validate_member(line) { out.insert(rel); }
            if out.len() > MAX_MANIFEST_MEMBERS { return Err(invalid("诊断成员数量超过上限")); }
        }
        Ok(out)
    }

    /// 进程内登记 ∪ 磁盘共享清单：同目录跨进程合作写者共同可见的成员集。
    fn effective_members(&self) -> io::Result<BTreeSet<PathBuf>> {
        let mut all = self.persisted_members()?;
        all.extend(self.members.iter().cloned());
        Ok(all)
    }

    /// 注册根运行期身份校验：路径被换成链接/重解析点或目录被替换都 fail-closed。
    fn verify_root(&self) -> io::Result<()> {
        if fs::symlink_metadata(&self.root).map(|m| m.file_type().is_symlink()).unwrap_or(true) {
            return Err(invalid("注册根被替换为链接/重解析点"));
        }
        let meta = fs::metadata(&self.root)
            .map_err(|e| io::Error::new(e.kind(), "注册根不可访问"))?;
        if !meta.is_dir() { return Err(invalid("注册根不再是目录")); }
        let handle = win::open_dir(&self.root)
            .map_err(|e| io::Error::new(e.kind(), "注册根无法打开目录句柄"))?;
        match (self.root_id, win::info(&handle).map(|i| (i.volume, i.index))) {
            (a, Some(b)) if a == b => Ok(()),
            // 身份不可验证（FS 不支持 file index）时 fail-closed，不静默放行。
            _ => Err(invalid("注册根身份不可验证或已变化")),
        }
    }

    /// 打开已验证成员用于追加：打开的句柄即被校验对象——最终成员是 symlink/重解析
    /// 时 canonical 解析出根外直接拒绝；硬链接别名拒绝（避免修改同盘外部同名文件）。
    /// 返回的 File 已通过：非重解析 + link_count==1 + canonical 在注册根内 +
    /// 句柄 file index 与 canonical 元数据一致。校验绑定句柄，成员替换无法再影响已开文件。
    fn open_append_verified(&self, rel: &Path) -> io::Result<File> {
        if !self.ancestors_clean(rel) {
            return Err(invalid("成员祖先目录含链接/重解析，拒绝写入"));
        }
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        // 以下校验全部作用于已打开句柄——路径此后被替换也无法偷换写入对象。
        let info = win::info(&file).ok_or_else(|| invalid("成员身份不可验证，拒绝写入"))?;
        if info.attrs & win::FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(invalid("成员为重解析点，拒绝写入"));
        }
        if info.links > 1 { return Err(invalid("成员存在硬链接别名，拒绝写入")); }
        // 成员当前解析的规范路径必须在根内；再开规范路径句柄比对 inode——
        // 若打开后路径被换，两句柄指向不同文件，file index 不一致即拒绝。
        let canonical = fs::canonicalize(&path)
            .map_err(|e| io::Error::new(e.kind(), "成员规范化失败"))?;
        if !path_within(&canonical, &self.root_canonical) {
            return Err(invalid("成员真实位置在注册根外，拒绝写入"));
        }
        let cfile = File::open(&canonical)
            .map_err(|e| io::Error::new(e.kind(), "规范路径无法打开验证句柄"))?;
        let cinfo = win::info(&cfile).ok_or_else(|| invalid("规范路径身份不可验证"))?;
        if (cinfo.volume, cinfo.index) != (info.volume, info.index) {
            return Err(invalid("成员句柄与规范路径身份不一致，拒绝写入"));
        }
        Ok(file)
    }

    /// 逐层验证成员各层父目录：存在、是目录、且非符号链接/重解析点。
    fn ancestors_clean(&self, member: &Path) -> bool {
        let mut current = self.root.clone();
        let mut components = member.components().peekable();
        while let Some(component) = components.next() {
            if components.peek().is_none() { break; } // 最后一段是文件本身
            current = current.join(component.as_os_str());
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
                _ => return false,
            }
        }
        true
    }

    fn member_size(&self, member: &Path) -> io::Result<u64> {
        if !self.ancestors_clean(member) { return Ok(0); }
        let path = self.root.join(member);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => Ok(0),
            Ok(meta) => Ok(meta.len()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }
}

/// Windows 句柄级文件身份：绑定已打开句柄，路径替换无法再影响校验对象。
/// 走 winapi-util 安全封装（crate 禁用 unsafe）。
#[cfg(windows)]
mod win {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::Path;
    use winapi_util::file::information;

    pub const FILE_ATTRIBUTE_REPARSE_POINT: u64 = 0x400;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    pub struct HandleInfo {
        pub volume: u64,
        pub index: u64,
        pub links: u64,
        pub attrs: u64,
    }

    /// BY_HANDLE_FILE_INFORMATION：卷序列号 + 文件索引 + 链接数 + 属性（句柄绑定）。
    pub fn info(file: &File) -> Option<HandleInfo> {
        let i = information(file).ok()?;
        Some(HandleInfo {
            volume: i.volume_serial_number(),
            index: i.file_index(),
            links: i.number_of_links(),
            attrs: i.file_attributes(),
        })
    }

    /// 目录句柄（FILE_FLAG_BACKUP_SEMANTICS 才能打开目录）。
    pub fn open_dir(path: &Path) -> io::Result<File> {
        OpenOptions::new().read(true).custom_flags(FILE_FLAG_BACKUP_SEMANTICS).open(path)
    }
}

#[cfg(not(windows))]
mod win {
    use std::fs::File;
    use std::io;
    use std::path::Path;
    pub const FILE_ATTRIBUTE_REPARSE_POINT: u64 = 0x400;
    pub struct HandleInfo { pub volume: u64, pub index: u64, pub links: u64, pub attrs: u64 }
    pub fn info(_: &File) -> Option<HandleInfo> { None }
    pub fn open_dir(path: &Path) -> io::Result<File> { File::open(path) }
}

/// child 是否位于 root 之下（组件级、大小写不敏感——NTFS 语义）。
fn path_within(child: &Path, root: &Path) -> bool {
    let mut it = child.components();
    for expected in root.components() {
        match it.next() {
            Some(actual)
                if actual.as_os_str().to_string_lossy()
                    .eq_ignore_ascii_case(&expected.as_os_str().to_string_lossy()) => {}
            _ => return false,
        }
    }
    true
}

fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, message.to_string()) }

/// 成员路径校验：只允许 Normal 组件的相对路径，拒绝绝对/前缀/父级/逃逸。
fn validate_member(relative: &str) -> io::Result<PathBuf> {
    if relative.is_empty() || relative.chars().count() > 480 {
        return Err(invalid("成员路径为空或过长"));
    }
    let candidate = Path::new(relative);
    if candidate.is_absolute() {
        return Err(invalid("成员路径必须相对注册根"));
    }
    let mut clean = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.starts_with('.') && part.chars().all(|c| c == '.') {
                    return Err(invalid("成员路径段非法"));
                }
                if part.chars().any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*')) {
                    return Err(invalid("成员路径含非法字符"));
                }
                clean.push(part.as_ref());
            }
            Component::CurDir => {}
            _ => return Err(invalid("成员路径不得含父级/根/前缀组件")),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(invalid("成员路径解析后为空"));
    }
    Ok(clean)
}

/// 进程内共享配额表：同一注册根下所有诊断写者共用成员集与总额预算，
/// 避免每路写者各自独立上限叠加导致目录总占用超限。
static SHARED_QUOTAS: OnceLock<Mutex<HashMap<PathBuf, DiagnosticQuota>>> = OnceLock::new();

/// 共享预算默认 64MiB；COOLZHU_DIAG_QUOTA_BYTES 可覆盖，0 或超过 4GiB 一律回退默认。
#[must_use]
pub fn default_budget_bytes() -> u64 {
    std::env::var("COOLZHU_DIAG_QUOTA_BYTES").ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|b| *b > 0 && *b <= 4 * 1024 * 1024 * 1024)
        .unwrap_or(64 * 1024 * 1024)
}

/// 生产写路径统一入口：以 path 父目录为注册根、登记其轮转族，
/// 先按（预算 - 本次写入）淘汰最旧成员，再在配额锁内检查+追加原子完成。
/// 根缺失/为链接、锁忙、成员身份不可验或写后仍超预算均 fail-closed 返回错误。
pub(crate) fn append_shared(path: &Path, record: &[u8], generations: usize) -> io::Result<()> {
    append_shared_with_budget(path, record, generations, default_budget_bytes())
}

pub(crate) fn append_shared_with_budget(
    path: &Path, record: &[u8], generations: usize, budget: u64,
) -> io::Result<()> {
    if record.is_empty() { return Ok(()); }
    let root = path.parent().ok_or_else(|| invalid("诊断文件缺少父目录"))?;
    let name = path.file_name().and_then(|n| n.to_str())
        .ok_or_else(|| invalid("诊断文件名不是有效 UTF-8"))?;
    let key = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let map = SHARED_QUOTAS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = map.lock().map_err(|_| io::Error::other("配额注册表锁已中毒"))?;
    if !map.contains_key(&key) {
        map.insert(key.clone(), DiagnosticQuota::new(root, budget)?);
    }
    let quota = map.get_mut(&key).expect("刚刚登记");
    if PRODUCT_LOG_FAMILIES.contains(&name) {
        // 新进程只接管源码明确列出的产品日志族；伪造的清单条目仍只计数，不可删除。
        for family in PRODUCT_LOG_FAMILIES { quota.register_rotated(family, generations)?; }
    }
    quota.register_rotated(name, generations)?;
    quota.enforce_with_headroom(record.len() as u64)?;
    quota.append_gated(name, record)
}

#[cfg(test)]
mod xproc {
    //! 跨进程协作写者的独立进程夹具：env 驱动，每个实例即一个真实进程。
    //! 运行方式：XPROC_ROOT=<dir> XPROC_NAME=a.log XPROC_BYTES=600 XPROC_BUDGET=1000 <test-exe> xproc_shared_writer --ignored
    use super::*;

    #[test]
    #[ignore]
    fn xproc_shared_writer() {
        let root = PathBuf::from(std::env::var("XPROC_ROOT").unwrap());
        let name = std::env::var("XPROC_NAME").unwrap();
        let bytes: usize = std::env::var("XPROC_BYTES").unwrap().parse().unwrap();
        let budget: u64 = std::env::var("XPROC_BUDGET").unwrap().parse().unwrap();
        let mut q = DiagnosticQuota::new(&root, budget).unwrap();
        q.register_rotated(&name, 3).unwrap();
        q.append_gated(&name, &vec![b'x'; bytes]).unwrap();
        // 本进程视角的登记成员总量——跨进程共享时应含对方成员，缺失即缺口。
        let accounted = q.accounted_bytes().unwrap();
        fs::write(root.join(format!("{name}.accounted")), accounted.to_string()).unwrap();
    }
}

#[cfg(test)]
#[path = "quota_key_acceptance.rs"]
mod key_acceptance;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn case_dir(name: &str) -> PathBuf {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::var("CARGO_TARGET_TMPDIR").map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"D:/coolzhuagent/tmp/goal-32wbs-acceptance-20261013/quota-274"))
            .join(format!("quota-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_at(path: &Path, bytes: usize, age_secs: u64) {
        fs::write(path, vec![b'q'; bytes]).unwrap();
        let when = SystemTime::now() - Duration::from_secs(age_secs);
        File::options().write(true).open(path).unwrap().set_modified(when).unwrap();
    }

    #[test]
    fn enforce_evicts_oldest_registered_only_until_under_budget() {
        let root = case_dir("evict");
        write_at(&root.join("a.log"), 4096, 100);
        write_at(&root.join("b.log"), 4096, 10);
        write_at(&root.join("keep-unregistered.log"), 8192, 1);
        write_at(&root.join("tool-audit.jsonl"), 8192, 1); // 权威旁路：未登记即不动
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("a.log").unwrap();
        quota.register_file("b.log").unwrap();
        let report = quota.enforce().unwrap();
        assert_eq!(report.total_before, 8192);
        assert_eq!(report.total_after, 4096);
        assert_eq!(report.evicted.len(), 1);
        assert!(report.evicted[0].0.ends_with("a.log"));
        assert!(!root.join("a.log").exists());
        assert_eq!(fs::metadata(root.join("b.log")).unwrap().len(), 4096);
        assert_eq!(fs::metadata(root.join("keep-unregistered.log")).unwrap().len(), 8192);
        assert_eq!(fs::metadata(root.join("tool-audit.jsonl")).unwrap().len(), 8192);
    }

    #[test]
    fn rotated_generations_evicted_oldest_first() {
        let root = case_dir("rotate");
        write_at(&root.join("diag.log"), 1000, 10);
        write_at(&root.join("diag.log.1"), 1000, 100);
        write_at(&root.join("diag.log.2"), 1000, 200);
        write_at(&root.join("diag.log.3"), 1000, 300);
        let mut quota = DiagnosticQuota::new(&root, 2000).unwrap();
        quota.register_rotated("diag.log", 3).unwrap();
        let report = quota.enforce().unwrap();
        assert_eq!(report.total_before, 4000);
        assert_eq!(report.total_after, 2000);
        let names: Vec<_> = report.evicted.iter().map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names, ["diag.log.3", "diag.log.2"]);
        assert!(root.join("diag.log").exists() && root.join("diag.log.1").exists());
    }

    #[test]
    fn register_rejects_escape_absolute_and_bad_components() {
        let root = case_dir("escape");
        let mut quota = DiagnosticQuota::new(&root, 1024).unwrap();
        for bad in ["../x.log", "a/../b.log", "", "C:/win/x.log", "\\\\share\\x", "evil:ads.log"] {
            assert!(quota.register_file(bad).is_err(), "{bad} 应拒绝");
        }
        assert!(quota.accounted_bytes().unwrap() == 0);
    }

    #[test]
    fn exact_budget_boundary_evicts_nothing() {
        let root = case_dir("boundary");
        write_at(&root.join("a.log"), 2048, 50);
        write_at(&root.join("b.log"), 2048, 10);
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("a.log").unwrap();
        quota.register_file("b.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(report.evicted.is_empty());
        assert_eq!(report.total_after, 4096);
        assert!(quota.would_exceed(1).unwrap());
        assert!(!quota.would_exceed(0).unwrap());
    }

    #[test]
    fn enforce_while_lock_held_fails_closed_and_deletes_nothing() {
        // 确定性并发：外部先占配额锁，enforce 必须立即失败且不动任何文件。
        let root = case_dir("locked");
        write_at(&root.join("a.log"), 8192, 100);
        let mut quota = DiagnosticQuota::new(&root, 1024).unwrap();
        quota.register_file("a.log").unwrap();
        let guard = OpenOptions::new().create(true).write(true).open(quota.lock_file_path()).unwrap();
        guard.try_lock().unwrap();
        let err = quota.enforce().unwrap_err();
        assert!(err.to_string().contains("锁"), "{err}");
        drop(guard);
        assert!(root.join("a.log").exists());
        // 释放后可正常执行
        let report = quota.enforce().unwrap();
        assert_eq!(report.total_after, 0);
    }

    #[test]
    fn active_window_member_never_evicted() {
        // 30s 内被写的成员视为活跃，即使总量超预算也不淘汰；只淘汰过窗成员。
        let root = case_dir("active");
        write_at(&root.join("active.log"), 4096, 5);
        write_at(&root.join("stale.log"), 4096, 300);
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("active.log").unwrap();
        quota.register_file("stale.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(root.join("active.log").exists(), "活跃成员必须保留");
        assert!(report.skipped.iter().any(|(_, r)| *r == "active-window"));
        assert_eq!(report.evicted.len(), 1);
        assert!(report.evicted[0].0.ends_with("stale.log"));
    }

    #[test]
    fn held_member_is_identity_unverified_not_evicted() {
        // 持有句柄的成员删除失败：记入 failed、继续淘汰其余、已删事实不丢。
        let root = case_dir("held");
        write_at(&root.join("held.log"), 4096, 300);
        write_at(&root.join("free.log"), 4096, 200);
        // Windows：share_mode(0) 独占句柄才能阻止 DeleteFile（std 默认允许 share-delete）
        use std::os::windows::fs::OpenOptionsExt;
        let _open = OpenOptions::new().read(true).share_mode(0).open(root.join("held.log")).unwrap();
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("held.log").unwrap();
        quota.register_file("free.log").unwrap();
        let report = quota.enforce().unwrap();
        // 独占句柄使身份不可验证：fail-closed 跳过（不淘汰），free.log 继续淘汰。
        assert!(report.skipped.iter().any(|(p, r)| *r == "identity-unverified" && p.ends_with("held.log")),
            "held.log 应记为 identity-unverified: {report:?}");
        assert!(report.evicted.iter().any(|(p, _)| p.ends_with("free.log")),
            "free.log 应被继续淘汰: {report:?}");
        assert!(root.join("held.log").exists());
        drop(_open);
    }

    #[test]
    fn manifest_hardlink_to_outside_sentinel_rejected_and_untouched() {
        // 反例：清单被预置为指向根外哨兵的硬链接——读写都必须拒绝，哨兵绝不被写入。
        let root = case_dir("mlink");
        let outside_dir = case_dir("mlink-outside");
        let outside = outside_dir.join("sentinel.log");
        fs::write(&outside, b"SENTINEL").unwrap();
        fs::hard_link(&outside, root.join(".diag-quota.members")).unwrap();
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        let err = quota.register_file("diag.log").unwrap_err();
        assert!(err.to_string().contains("硬链接"), "{err}");
        assert_eq!(fs::read(&outside).unwrap(), b"SENTINEL", "哨兵文件绝不可被写入");
        assert!(quota.enforce().is_err(), "读侧同样 fail-closed");
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside_dir).unwrap();
    }

    #[test]
    fn manifest_forged_authoritative_name_never_evicted_and_write_rejected() {
        // 损坏/伪造清单注入权威文件名：计入占用、永不淘汰、写入仍按未登记拒绝。
        let root = case_dir("forged");
        fs::write(root.join(".diag-quota.members"), "tool-audit.jsonl
").unwrap();
        write_at(&root.join("tool-audit.jsonl"), 4096, 300);
        write_at(&root.join("own.log"), 4096, 300);
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("own.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(root.join("tool-audit.jsonl").exists(), "清单注入名绝不可被淘汰");
        assert!(report.skipped.iter().any(|(p, r)| *r == "foreign-registration" && p.ends_with("tool-audit.jsonl")), "{report:?}");
        assert!(report.evicted.iter().all(|(p, _)| p.ends_with("own.log")), "只允许淘汰本进程登记成员: {report:?}");
        assert!(quota.append_gated("tool-audit.jsonl", b"x").is_err(), "未登记成员写入必须拒绝");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn append_gated_serializes_writers_never_exceeds_budget() {
        // 两个并发写者共用同一锁的检查+追加：尝试总量远超预算，最终总量不越界，
        // 越界写入一律拒绝而非部分写盘。
        let root = case_dir("gated");
        let mut quota = DiagnosticQuota::new(&root, 1000).unwrap();
        quota.register_file("shared.log").unwrap();
        let quota = std::sync::Arc::new(quota);
        let mut handles = Vec::new();
        for tag in ["A", "B"] {
            let q = quota.clone();
            handles.push(std::thread::spawn(move || {
                let mut ok = 0; let mut refused = 0;
                for i in 0..10 {
                    let rec = format!("{tag}-{i}-{}
", "x".repeat(180));
                    match q.append_gated("shared.log", rec.as_bytes()) {
                        Ok(()) => ok += 1,
                        Err(_) => refused += 1,
                    }
                }
                (ok, refused)
            }));
        }
        let (a, b) = (handles.remove(0).join().unwrap(), handles.remove(0).join().unwrap());
        let size = fs::metadata(root.join("shared.log")).map(|m| m.len()).unwrap_or(0);
        assert!(size <= 1000, "总量绝不超预算: {size}");
        assert!(a.1 + b.1 > 0, "超额尝试必须被拒绝: {a:?} {b:?}");
    }

    #[test]
    fn append_gated_rejects_unregistered_and_overbudget() {
        let root = case_dir("gated-neg");
        let mut quota = DiagnosticQuota::new(&root, 512).unwrap();
        quota.register_file("known.log").unwrap();
        assert!(quota.append_gated("stray.log", b"x").is_err(), "未登记成员必须拒绝");
        assert!(!root.join("stray.log").exists());
        assert!(quota.append_gated("known.log", &vec![b'x'; 600]).is_err(), "超预算写入必须拒绝");
        assert!(!root.join("known.log").exists(), "拒绝的写入不得产生文件");
    }

    #[cfg(windows)]
    #[test]
    fn append_gated_linked_member_never_writes_outside() {
        let root = case_dir("app-linked");
        let outside = case_dir("app-linked-out");
        fs::write(outside.join("victim.log"), b"OUTSIDE-276").unwrap();
        let mut quota = DiagnosticQuota::new(&root, 8192).unwrap();
        if std::os::windows::fs::symlink_file(outside.join("victim.log"), root.join("member.log")).is_ok() {
            quota.register_file("member.log").unwrap();
            assert!(quota.append_gated("member.log", b"PAYLOAD").is_err(),
                "file-symlink member must be refused");
        } else {
            let plain = |p: &Path| p.to_string_lossy().replace('/', "\\");
            let st = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(plain(&root.join("evil"))).arg(plain(&outside))
                .status().expect("mklink spawn");
            assert!(st.success(), "junction creation failed");
            quota.register_file("evil/victim.log").unwrap();
            assert!(quota.append_gated("evil/victim.log", b"PAYLOAD").is_err(),
                "junction member must be refused");
        }
        assert_eq!(fs::read(outside.join("victim.log")).unwrap(), b"OUTSIDE-276");
    }

    #[cfg(windows)]
    #[test]
    fn append_gated_hardlink_member_never_writes_outside() {
        // 硬链接成员指向根外同文件：追加会同时改根外内容——必须拒绝，两边字节都不变。
        let root = case_dir("app-hardlink");
        let outside = case_dir("app-hardlink-out");
        fs::write(outside.join("authoritative.log"), b"AUTH-276").unwrap();
        fs::hard_link(outside.join("authoritative.log"), root.join("member.log")).unwrap();
        let mut quota = DiagnosticQuota::new(&root, 8192).unwrap();
        quota.register_file("member.log").unwrap();
        assert!(quota.append_gated("member.log", b"X").is_err(), "硬链接成员必须拒绝");
        assert_eq!(fs::read(outside.join("authoritative.log")).unwrap(), b"AUTH-276");
        assert_eq!(fs::read(root.join("member.log")).unwrap(), b"AUTH-276");
    }

    #[cfg(windows)]
    #[test]
    fn append_gated_nested_junction_ancestor_refused() {
        // 成员祖先为 junction：append_gated 拒绝，不创建也不写根外文件。
        let root = case_dir("app-junction");
        let outside = case_dir("app-junction-out");
        fs::create_dir_all(outside.join("inner")).unwrap();
        let plain = |p: &Path| p.to_string_lossy().replace('/', "\\");
        let st = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(plain(&root.join("evil"))).arg(plain(&outside.join("inner")))
            .status().expect("mklink spawn");
        assert!(st.success(), "junction creation failed");
        let mut quota = DiagnosticQuota::new(&root, 8192).unwrap();
        quota.register_file("evil/x.log").unwrap();
        assert!(quota.append_gated("evil/x.log", b"X").is_err(), "嵌套链接成员必须拒绝");
        assert!(!outside.join("inner/x.log").exists(), "根外不得产生文件");
    }

    #[cfg(windows)]
    #[test]
    fn root_swapped_at_runtime_fails_closed() {
        // root 运行期被换成指向别处的 junction：enforce 与 append_gated 都 fail-closed。
        let root = case_dir("root-swap");
        write_at(&root.join("a.log"), 1024, 100);
        let moved = root.with_file_name(format!("{}-moved", root.file_name().unwrap().to_string_lossy()));
        let outside = case_dir("root-swap-out");
        fs::rename(&root, &moved).unwrap();
        let plain = |p: &Path| p.to_string_lossy().replace('/', "\\");
        let st = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(plain(&root)).arg(plain(&outside))
            .status().expect("mklink spawn");
        assert!(st.success(), "junction creation failed");
        let mut quota = DiagnosticQuota::new(&moved, 1024).unwrap();
        quota.register_file("a.log").unwrap();
        // quota 指向被移动后的真实目录——换一个持有原路径 root 的实例更贴近攻击场景：
        // 原 root 现在是 junction，因此直接在原 root 上 new() 也应拒绝：
        assert!(DiagnosticQuota::new(&root, 1024).is_err(), "junction root 必须拒绝");
        // 运行期：对 moved 实例做 enforce（root 已移走不再占位）……改测 moved 自身被换：
        let moved2 = moved.with_file_name(format!("{}-2", moved.file_name().unwrap().to_string_lossy()));
        fs::rename(&moved, &moved2).unwrap();
        let st2 = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(plain(&moved)).arg(plain(&outside))
            .status().expect("mklink spawn");
        assert!(st2.success());
        assert!(quota.enforce().is_err(), "root 被换后 enforce 必须失败");
        assert!(quota.append_gated("a.log", b"X").is_err(), "root 被换后 append 必须失败");
        assert!(outside.join("a.log").exists() == false || fs::metadata(&outside.join("a.log")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn enforce_skips_hardlink_member_and_outside_intact() {
        // 超预算场景：硬链接成员跳过并记录，只淘汰普通陈旧成员，根外内容不变。
        let root = case_dir("enforce-hardlink");
        let outside = case_dir("enforce-hardlink-out");
        fs::write(outside.join("auth.log"), vec![b'a'; 4096]).unwrap();
        fs::hard_link(outside.join("auth.log"), root.join("linked.log")).unwrap();
        write_at(&root.join("stale.log"), 4096, 300);
        let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
        quota.register_file("linked.log").unwrap();
        quota.register_file("stale.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(report.skipped.iter().any(|(p, r)| *r == "hardlink-alias" && p.ends_with("linked.log")),
            "硬链接成员必须被跳过: {report:?}");
        assert!(root.join("linked.log").exists(), "硬链接成员不得被淘汰");
        assert!(outside.join("auth.log").exists());
        assert!(report.evicted.iter().any(|(p, _)| p.ends_with("stale.log")));
    }

    #[cfg(windows)]
    #[test]
    fn nested_link_member_never_deleted_and_outside_untouched() {
        // Windows 专属：注册根下嵌套目录链接指向根外；登记其成员时
        // 祖先校验 fail-closed，跳过并保留，绝不删除根外文件。
        let root = case_dir("nested-link");
        let outside = case_dir("outside-victim");
        fs::create_dir_all(outside.join("inner")).unwrap();
        fs::write(outside.join("inner/victim.log"), b"OUTSIDE-KEEP-275").unwrap();
        let junction = root.join("evil");
        // 目录 junction 不需要符号链接特权；失败即失败，不假装通过。
        let plain = |p: &Path| p.to_string_lossy().replace('/', "\\").to_string();
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J", &plain(&junction), &plain(&outside.join("inner"))])
            .status()
            .expect("mklink /J spawn failed");
        assert!(status.success(), "junction creation failed: {status}");
        assert!(junction.exists());
        let mut quota = DiagnosticQuota::new(&root, 1024).unwrap();
        quota.register_file("evil/victim.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(report.evicted.is_empty(), "嵌套链接成员不得淘汰");
        assert!(report.skipped.iter().any(|(_, r)| *r == "ancestor-unverified"), "{report:?}");
        assert_eq!(fs::read(outside.join("inner/victim.log")).unwrap(), b"OUTSIDE-KEEP-275");
    }

    #[cfg(windows)]
    #[test]
    fn symlink_member_is_skipped_never_deleted() {
        // Windows 专属：符号链接/重解析点成员跳过（不跟随、不删除目标）。
        let root = case_dir("symlink");
        let real = root.join("real.log");
        write_at(&real, 4096, 5);
        let link = root.join("linked.log");
        if std::os::windows::fs::symlink_file(&real, &link).is_err() {
            eprintln!("symlink unavailable, skipping");
            return;
        }
        let mut quota = DiagnosticQuota::new(&root, 1024).unwrap();
        quota.register_file("linked.log").unwrap();
        let report = quota.enforce().unwrap();
        assert!(report.evicted.is_empty());
        assert!(report.skipped.iter().any(|(_, r)| *r == "symlink-or-reparse"));
        assert!(link.exists() && fs::metadata(&real).unwrap().len() == 4096);
    }
}
