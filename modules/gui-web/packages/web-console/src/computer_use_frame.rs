//! **CU-04 帧绑定**：把"模型给出的坐标究竟属于哪一版实际图像、哪次裁剪、哪个缩放，
//! 以及它们如何映射到物理位置"变成**可记录、可解析、可核对**的事实。
//!
//! ## 为什么不是"再加一个 frame_id"
//!
//! 审查既有绑定（`computer_use_desktop_bridge.rs` / `computer_use_adapters.rs`）后的结论是：
//! 需要的身份**已经都在**，缺的是"把它们绑在一起并留下可核对的记录"：
//!
//! | 需要的绑定 | 既有载体 |
//! | --- | --- |
//! | 观察代次（哪一次观察） | `Observation.generation` → `computer_use_steps.observation_generation` |
//! | 图像版本（哪一张图） | `image.sha256`（helper 的 .NET SHA256） |
//! | 缩放／尺寸（多大一张图） | `image.width` / `image.height` |
//! | 裁剪（图对应屏幕哪一块） | `image.screen_rect`（截图在屏幕物理像素上的原点+尺寸） |
//! | 坐标容器（0..1 相对谁） | `canvas_rect`（= `client_rect ∩ screen_rect`）或目标元素 rect |
//!
//! 因此本模块**不引入独立 ID 空间**：`FrameRef` 只是上面这些既有值的不可变组合，
//! 其证据串复用与 `screenshot:` 完全相同的摘要+尺寸词汇，使两者能互相核对。
//!
//! ## 既有事实（审查结果，避免重复劳动与过度声明）
//!
//! 会携带**模型坐标**的动作只有 `Drag`（0..1 归一化点）。而 `Drag` 的输入前守卫**已经**
//! 比较了：窗口身份五项（hwnd／pid／rect／dpi／webview2）、截图画布的内容摘要、
//! UIA 画布的边界与身份。本模块**不改动**这些既有比较，只补上它们没有覆盖的部分：
//!
//! 1. **绑定本身没有被记录**：`screenshot:<path>:sha256=<d>:<W>x<H>` 全仓库**只有生产端、
//!    没有解析端**，"坐标属于哪一版图"事后无法核对。
//! 2. **坐标容器与证据图像之间的一致性没有被核对**：两者若来自不同快照（现在不会，
//!    但没有任何东西阻止将来的重构造成这种错配），坐标会被映射到错误的裁剪/缩放下而不报错。
//! 3. **几何缺失时没有明确表达**：拿不到 sha256／`screen_rect`／`canvas_rect` 时，
//!    现在的代码不会拒绝，也就无法区分"绑定成立"与"根本没绑定"。
//!
//! ## 映射口径（与 `stroke_arguments` 同一份约定，在此显式化为可测函数）
//!
//! 归一化点 `(x, y) ∈ [0,1]²` 映射到 `canvas_rect` 内的物理像素：
//! `px = rect[0] + round(x * (rect[2] - 1))`。这正是 `stroke_arguments` 用的式子，
//! 本模块用 [`FrameRef::map_unit_point`] 把它固定下来，避免"文档说一套、代码做一套"。

use serde_json::Value;

/// 帧绑定证据串的前缀。与 `screenshot:` 同族，便于一起检索与核对。
pub(crate) const FRAME_BINDING_PREFIX: &str = "frame_binding:";

/// 一次观察的**图像版本 + 裁剪 + 缩放**身份，以及归一化坐标的容器。
///
/// 所有字段都取自既有数据，没有任何新造标识；构造后不可变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameRef {
    /// helper 计算的图像内容摘要（.NET SHA256，小写十六进制）。
    pub image_sha256: String,
    /// 图像宽高（即裁剪后的像素尺寸）。
    pub image_width: u32,
    pub image_height: u32,
    /// 截图在屏幕物理像素上的原点与尺寸：坐标由此回到物理位置。
    pub screen_rect: [i32; 4],
    /// 归一化坐标的容器（`client_rect ∩ screen_rect`，或目标元素 rect 的物理像素）。
    pub canvas_rect: [i32; 4],
}

/// 无法建立绑定时如实列出缺了什么——**不猜、不补默认值**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameUnbindable {
    pub missing: Vec<&'static str>,
    pub detail: String,
}

impl FrameUnbindable {
    fn new(missing: Vec<&'static str>, detail: impl Into<String>) -> Self {
        Self {
            missing,
            detail: detail.into(),
        }
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "无法建立帧绑定（缺少 {}）：{}",
            self.missing.join("、"),
            self.detail
        )
    }
}

/// 绑定不成立的具体种类。**可分辨**是重点：调用方与审计都要能区分
/// "图像变了"／"缩放变了"／"裁剪变了"，而不是笼统一句"不一致"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameMismatch {
    /// 同一尺寸但内容不同 ⇒ 换了一张图。
    WrongImage,
    /// 图像尺寸不同 ⇒ 缩放/裁剪范围不同。
    WrongScale,
    /// 截图对应的屏幕区域，或坐标容器所在区域不同 ⇒ 裁剪基准不同。
    WrongCrop,
}

impl FrameMismatch {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::WrongImage => "wrong_image",
            Self::WrongScale => "wrong_scale",
            Self::WrongCrop => "wrong_crop",
        }
    }

    pub(crate) fn reason(self) -> &'static str {
        match self {
            Self::WrongImage => "坐标所属图像的内容摘要与本次绑定的图像不一致（错误的图像）",
            Self::WrongScale => "坐标所属图像的尺寸与本次绑定的图像不一致（错误的缩放）",
            Self::WrongCrop => "坐标所属截图的屏幕区域或坐标容器与本次绑定不一致（错误的裁剪）",
        }
    }
}

/// 绑定失败的两种可分辨结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FrameBindingError {
    /// 几何信息不全：不能建立绑定（不是"绑定失败"，而是"根本没绑上"）。
    Unbindable(FrameUnbindable),
    /// 信息齐全但自相矛盾：这是真正的错配，必须拒绝。
    Mismatch(FrameMismatch),
}

impl FrameBindingError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Unbindable(_) => "frame_unbindable",
            Self::Mismatch(mismatch) => mismatch.code(),
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Unbindable(unbindable) => unbindable.describe(),
            Self::Mismatch(mismatch) => mismatch.reason().to_string(),
        }
    }

    /// 把"没绑上／绑错"**如实**写成证据串（与成功绑定同族前缀，便于一起检索）。
    ///
    /// 关键：`unbindable` 不是失败的同义词，而是"这份观察没有可用于绑定的几何"——
    /// 必须与"绑错了"分开记录，否则事后无法区分"没有绑定"与"绑定不成立"。
    pub(crate) fn evidence(&self) -> String {
        match self {
            Self::Unbindable(unbindable) => format!(
                "{FRAME_BINDING_PREFIX}unbindable:missing={}",
                if unbindable.missing.is_empty() {
                    "unknown".to_string()
                } else {
                    unbindable.missing.join("|")
                }
            ),
            Self::Mismatch(mismatch) => {
                format!("{FRAME_BINDING_PREFIX}mismatch:code={}", mismatch.code())
            }
        }
    }
}

/// `screenshot:` 证据串解析出的图像身份（摘要 + 尺寸）。
///
/// 这个解析函数的存在本身就是修复项：该字符串此前**只写不读**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScreenshotEvidence {
    pub sha256: String,
    pub width: u32,
    pub height: u32,
}

/// 解析 `screenshot:<path>:sha256=<digest>:<W>x<H>`。
///
/// 宽容之处只在路径：路径可能含冒号（`C:\...`），因此从**右侧**按已知字段回退匹配；
/// 摘要与尺寸必须严格是 `<64 hex>` 与 `<u32>x<u32>`，否则返回 `None`（不猜）。
pub(crate) fn parse_screenshot_evidence(value: &str) -> Option<ScreenshotEvidence> {
    let body = value.strip_prefix("screenshot:")?;
    let marker = ":sha256=";
    let index = body.rfind(marker)?;
    let (_, tail) = body.split_at(index + marker.len());
    let (sha256, size) = tail.split_once(':')?;
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let (width, height) = size.split_once('x')?;
    Some(ScreenshotEvidence {
        sha256: sha256.to_ascii_lowercase(),
        width: width.parse().ok()?,
        height: height.parse().ok()?,
    })
}

/// 从 `[x,y,w,h]` JSON 读矩形；形状不对就 `None`（不猜）。
///
/// 供本模块与适配器共享：两侧必须用同一套读取规则，否则容器比较会失去意义。
pub(crate) fn rect_from_json(value: &Value) -> Option<[i32; 4]> {
    let values = value.as_array().filter(|values| values.len() == 4)?;
    let mut rect = [0i32; 4];
    for (index, item) in values.iter().enumerate() {
        rect[index] = i32::try_from(item.as_i64()?).ok()?;
    }
    Some(rect)
}

fn contains_rect(outer: [i32; 4], inner: [i32; 4]) -> bool {
    let (ox, oy) = (i64::from(outer[0]), i64::from(outer[1]));
    let (iw, ih) = (i64::from(inner[2]), i64::from(inner[3]));
    if iw <= 0 || ih <= 0 {
        return false;
    }
    let (ix, iy) = (i64::from(inner[0]), i64::from(inner[1]));
    let (ow, oh) = (i64::from(outer[2]), i64::from(outer[3]));
    ix >= ox && iy >= oy && ix + iw <= ox + ow && iy + ih <= oy + oh
}

impl FrameRef {
    /// 从快照的 `state` + 证据列表建立绑定，并**顺带核对**两者是否描述同一张图。
    ///
    /// - `state` 提供 `image.sha256`／`width`／`height`／`screen_rect`；
    /// - `container` 是**归一化坐标实际相对的容器**（拖拽的 `canvas_rect`，或目标元素 rect）——
    ///   由调用方给出，因为它取决于具体动作，不能由快照单方面假定；
    /// - `evidence` 里的 `screenshot:` 串提供**独立来源**的摘要与尺寸。
    ///
    /// 两个来源必须一致：这正是"坐标容器与证据图像是同一版"的可核对形式。
    pub(crate) fn bind(
        state: &Value,
        evidence: &[String],
        container: [i32; 4],
    ) -> Result<Self, FrameBindingError> {
        let mut missing = Vec::new();
        let sha256 = state
            .pointer("/image/sha256")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .filter(|value| !value.is_empty());
        if sha256.is_none() {
            missing.push("image.sha256");
        }
        let width = state
            .pointer("/image/width")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        if width.is_none() {
            missing.push("image.width");
        }
        let height = state
            .pointer("/image/height")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok());
        if height.is_none() {
            missing.push("image.height");
        }
        let screen_rect = state.pointer("/image/screen_rect").and_then(rect_from_json);
        if screen_rect.is_none() {
            missing.push("image.screen_rect");
        }
        if !missing.is_empty() {
            return Err(FrameBindingError::Unbindable(FrameUnbindable::new(
                missing,
                "快照缺少这些字段时无法说明坐标属于哪一版图像与哪次裁剪/缩放，\
                 因此不建立绑定，也不以推测值代替",
            )));
        }
        let screenshot = evidence
            .iter()
            .find_map(|item| parse_screenshot_evidence(item));
        let Some(screenshot) = screenshot else {
            return Err(FrameBindingError::Unbindable(FrameUnbindable::new(
                vec!["screenshot 证据"],
                "证据列表里没有可解析的 screenshot:<path>:sha256=<d>:<W>x<H>，无法与图像互相核对",
            )));
        };
        let frame = Self {
            image_sha256: sha256.expect("已确认存在"),
            image_width: width.expect("已确认存在"),
            image_height: height.expect("已确认存在"),
            screen_rect: screen_rect.expect("已确认存在"),
            canvas_rect: container,
        };
        // 独立来源互查：state 里的图像身份必须与证据串一致。
        if frame.image_sha256 != screenshot.sha256 {
            return Err(FrameBindingError::Mismatch(FrameMismatch::WrongImage));
        }
        if (frame.image_width, frame.image_height) != (screenshot.width, screenshot.height) {
            return Err(FrameBindingError::Mismatch(FrameMismatch::WrongScale));
        }
        Ok(frame)
    }

    /// 坐标容器是否落在截图区域内。
    ///
    /// **注意口径**：这不是新的拒绝条件。拖拽路径在映射前**已经**用
    /// `intersect_rect(rect, screen_rect) != rect` 拒绝过容器越界（`target_offscreen`），
    /// 这里只把同一事实**记录**下来，供证据串与审计使用，避免重复拒绝、也不放松既有检查。
    pub(crate) fn container_inside_image(&self) -> bool {
        contains_rect(self.screen_rect, self.canvas_rect)
    }

    /// 把绑定写成证据串（进既有 `evidence` 通道，**不新增表/列**）。
    pub(crate) fn evidence(&self) -> String {
        format!(
            "{FRAME_BINDING_PREFIX}sha256={}:{}x{}:screen_rect={},{},{},{}:canvas_rect={},{},{},{}:container_inside_image={}",
            self.image_sha256,
            self.image_width,
            self.image_height,
            self.screen_rect[0],
            self.screen_rect[1],
            self.screen_rect[2],
            self.screen_rect[3],
            self.canvas_rect[0],
            self.canvas_rect[1],
            self.canvas_rect[2],
            self.canvas_rect[3],
            self.container_inside_image(),
        )
    }

    /// 读回绑定（把"只写不读"变成"可核对"）。
    pub(crate) fn parse(evidence: &str) -> Option<Self> {
        let body = evidence.strip_prefix(FRAME_BINDING_PREFIX)?;
        let mut sha256 = None;
        let mut size = None;
        let mut screen_rect = None;
        let mut canvas_rect = None;
        for field in split_fields(body) {
            let Some((key, value)) = field.split_once('=') else {
                // 尺寸字段**无名**（沿用 `screenshot:` 的 `<W>x<H>` 写法），只按形状识别。
                if size.is_none() {
                    size = parse_size(field);
                }
                continue;
            };
            match key {
                "sha256" => {
                    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
                        sha256 = Some(value.to_ascii_lowercase());
                    }
                }
                "screen_rect" => screen_rect = rect_from_csv(value),
                "canvas_rect" => canvas_rect = rect_from_csv(value),
                // 其余未知字段（如 container_inside_image）一律忽略，不参与绑定判定。
                _ => {}
            }
        }
        let (width, height) = size?;
        Some(Self {
            image_sha256: sha256?,
            image_width: width,
            image_height: height,
            screen_rect: screen_rect?,
            canvas_rect: canvas_rect?,
        })
    }

    /// 两个绑定之间的差异，按图像 → 缩放 → 裁剪的顺序给出**第一个**可分辨原因。
    ///
    /// 裁剪这一层同时看 `screen_rect`（截图对应屏幕哪一块）与 `canvas_rect`（0..1 相对谁）：
    /// 两者任一变化都会改变同一组归一化点的物理落点。**这是本模块真正补上的那一格**——
    /// 窗口矩形不变、图像内容不变时，`client_rect` 仍可能变化 ⇒ 容器变、落点变，
    /// 而既有守卫只比内容摘要，看不出这一类差异。
    pub(crate) fn classify(&self, other: &Self) -> Option<FrameMismatch> {
        if self.image_sha256 != other.image_sha256 {
            return Some(FrameMismatch::WrongImage);
        }
        if (self.image_width, self.image_height) != (other.image_width, other.image_height) {
            return Some(FrameMismatch::WrongScale);
        }
        if self.screen_rect != other.screen_rect || self.canvas_rect != other.canvas_rect {
            return Some(FrameMismatch::WrongCrop);
        }
        None
    }

    /// 归一化坐标 → 物理像素（容器 = `canvas_rect`）。
    ///
    /// 与 `stroke_arguments` 共用 [`unit_axis_to_pixel`]，因此钉住它的测试钉的是**同一份**
    /// 生产公式；`stroke_arguments` 自身的端点行为另有
    /// `desktop_stroke_relative_points_stay_inside_physical_bounds` 直接钉住。
    #[cfg(test)]
    pub(crate) fn map_unit_point(&self, x: f64, y: f64) -> (i32, i32) {
        (
            unit_axis_to_pixel(x, self.canvas_rect[0], self.canvas_rect[2]),
            unit_axis_to_pixel(y, self.canvas_rect[1], self.canvas_rect[3]),
        )
    }
}

/// 单个轴的归一化 → 物理像素换算：`offset + round(value * (size - 1))`。
///
/// **这是生产映射的唯一实现**：`stroke_arguments`（真实笔画路径）与本模块的
/// `FrameRef::map_unit_point` 都调用它。用 `size - 1` 是为了让 `1.0` 命中容器最后
/// 一个像素而不是越界一格——两侧端点都不越界。
pub(crate) fn unit_axis_to_pixel(value: f64, offset: i32, size: i32) -> i32 {
    offset + (value * f64::from(size - 1)).round() as i32
}

fn split_fields(body: &str) -> Vec<&str> {
    body.split(':').collect()
}

/// 只接受 `<u32>x<u32>` 形状，避免把无关字段误当尺寸。
fn parse_size(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    if width.is_empty() || height.is_empty() {
        return None;
    }
    if !width.bytes().all(|c| c.is_ascii_digit()) || !height.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((width.parse().ok()?, height.parse().ok()?))
}

fn rect_from_csv(value: &str) -> Option<[i32; 4]> {
    let parts = value.split(',').collect::<Vec<_>>();
    if parts.len() != 4 {
        return None;
    }
    let mut rect = [0i32; 4];
    for (index, part) in parts.iter().enumerate() {
        rect[index] = part.parse().ok()?;
    }
    Some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CANVAS: [i32; 4] = [110, 80, 700, 500];

    fn state(sha256: &str, width: u64, height: u64) -> Value {
        json!({
            "image": {
                "sha256": sha256,
                "width": width,
                "height": height,
                "screen_rect": [100, 50, 800, 600],
                "path": "captures\\window-1.png",
            },
            "canvas_rect": CANVAS,
        })
    }

    fn evidence(sha256: &str, width: u64, height: u64) -> String {
        format!("screenshot:C:\\e\\window-1.png:sha256={sha256}:{width}x{height}")
    }

    const DIGEST_A: &str = "8b05f056867f9577b6e8a34dbb9940d11535064c29da57617550bd9855239a5d";
    const DIGEST_B: &str = "73009e4257243de01b0b2c8745f5b5e44ed736da3358a73799e723ac09db66c5";

    /// 验收 6：既有 `screenshot:` 串从"只写不读"变成可解析（这是修复项本身）。
    #[test]
    fn screenshot_evidence_is_now_parseable() {
        let parsed = parse_screenshot_evidence(&evidence(DIGEST_A, 2560, 1152)).expect("可解析");
        assert_eq!(parsed.sha256, DIGEST_A);
        assert_eq!((parsed.width, parsed.height), (2560, 1152));
        // 摘要不是 64 hex ⇒ 明确拒绝，而不是当成摘要用。
        assert!(parse_screenshot_evidence("screenshot:p.png:sha256=abc:2x2").is_none());
        // 前缀不对（例如本模块自己的证据串）不得被当成截图证据。
        assert!(parse_screenshot_evidence(&format!("{FRAME_BINDING_PREFIX}sha256={DIGEST_A}:2x2")).is_none());
    }

    /// 验收 1（正向）：几何齐全且两个来源一致时绑定成立，且证据串可读回同一绑定。
    #[test]
    fn well_formed_observation_binds_and_round_trips() {
        let frame = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
            .expect("应能建立绑定");
        let round_trip = FrameRef::parse(&frame.evidence()).expect("证据串必须可读回");
        assert_eq!(round_trip, frame, "读回的绑定必须与写入的完全相同");
        assert_eq!(frame.classify(&round_trip), None, "自比对不得报差异");
    }

    /// 验收 2：**错误的图像**——摘要不同（尺寸相同）必须被拒绝且原因可分辨。
    #[test]
    fn wrong_image_is_rejected_and_distinguishable() {
        let error = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_B, 2560, 1152)], CANVAS)
            .expect_err("摘要不同不得建立绑定");
        assert_eq!(error.code(), "wrong_image");
        assert!(error.describe().contains("错误的图像"));
    }

    /// 验收 3：**错误的缩放**——尺寸不同必须被拒绝且原因可分辨（摘要相同也不行）。
    #[test]
    fn wrong_scale_is_rejected_and_distinguishable() {
        let error = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 1280, 720)], CANVAS)
            .expect_err("尺寸不同不得建立绑定");
        assert_eq!(error.code(), "wrong_scale");
        assert!(error.describe().contains("错误的缩放"));
    }

    /// 验收 4：**错误的裁剪**——截图对应的屏幕区域不同，按裁剪归类（不是图像、也不是缩放）。
    #[test]
    fn wrong_crop_is_classified_as_crop_not_image_or_scale() {
        let left = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
            .expect("绑定");
        let mut right = left.clone();
        right.screen_rect = [1000, 50, 800, 600];
        assert_eq!(left.classify(&right), Some(FrameMismatch::WrongCrop));
        assert_eq!(
            left.classify(&right).map(FrameMismatch::code),
            Some("wrong_crop")
        );
        // 顺序性：图像差异优先于裁剪差异报告（先修最根本的那一层）。
        let mut both = right.clone();
        both.image_sha256 = DIGEST_B.to_string();
        assert_eq!(left.classify(&both), Some(FrameMismatch::WrongImage));
    }

    /// 验收 5：容器是否落在截图区域内必须被**记录**下来。
    ///
    /// 口径说明：这不是新增拒绝条件——拖拽路径在映射前已用
    /// `intersect_rect(rect, screen_rect) != rect` 拒绝过越界容器（`target_offscreen`）。
    /// 本测试钉住的是"同一事实会被如实记进绑定"，既不重复拒绝、也不放松既有检查。
    #[test]
    fn container_containment_is_recorded_and_crop_divergence_is_classified() {
        let inside = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
            .expect("绑定");
        assert!(inside.container_inside_image(), "容器在截图内");
        assert!(inside.evidence().contains("container_inside_image=true"));

        // 容器与截图只有部分相交（画布被拖到屏幕外）⇒ 如实记为 false，且仍能建立绑定。
        let partial = FrameRef::bind(
            &state(DIGEST_A, 2560, 1152),
            &[evidence(DIGEST_A, 2560, 1152)],
            [400, 300, 700, 500],
        )
        .expect("部分相交仍可建立绑定");
        assert!(!partial.container_inside_image(), "部分相交必须记为 false");
        assert!(partial.evidence().contains("container_inside_image=false"));

        // 裁剪差异按裁剪归类；容器差异也归到裁剪这一层（因为它改变了 0..1 的物理落点）。
        let mut moved = inside.clone();
        moved.canvas_rect = [200, 100, 700, 500];
        assert_eq!(inside.classify(&moved), Some(FrameMismatch::WrongCrop));
        assert_eq!(
            FrameMismatch::WrongCrop.code(),
            "wrong_crop",
            "裁剪必须有自己的可分辨代码"
        );
    }

    /// 验收 7 附：证据串必须区分"**没绑上**"与"**绑错了**"，且"没绑上"不得被读成绑定。
    ///
    /// 这条直接支撑适配器侧的用法：适配器用 `FrameRef::parse` 读观察当时记下的绑定，
    /// 读不到（没绑上）就跳过比较；若 `unbindable` 被误解析成某个绑定，就会拿一份
    /// 虚构的帧去做比较，把"没有依据"伪装成"有依据"。
    #[test]
    fn unbindable_and_mismatch_markers_are_distinguishable_and_never_parse_as_bindings() {
        let unbindable = FrameBindingError::Unbindable(FrameUnbindable::new(
            vec!["image.sha256", "image.screen_rect"],
            "测试",
        ));
        assert_eq!(unbindable.code(), "frame_unbindable");
        let marker = unbindable.evidence();
        assert!(marker.starts_with(FRAME_BINDING_PREFIX));
        assert!(marker.contains("unbindable"));
        assert!(marker.contains("image.sha256|image.screen_rect"), "{marker}");
        assert!(
            FrameRef::parse(&marker).is_none(),
            "没绑上的标记不得被读成一个绑定"
        );

        let mismatch = FrameBindingError::Mismatch(FrameMismatch::WrongCrop);
        assert_eq!(mismatch.code(), "wrong_crop");
        let marker = mismatch.evidence();
        assert!(marker.contains("mismatch"));
        assert!(marker.contains("wrong_crop"), "{marker}");
        assert!(FrameRef::parse(&marker).is_none(), "绑错的标记不得被读成绑定");

        // 真正绑上时 marker 必须可读回，且与源绑定一致。
        let frame = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
            .expect("绑定");
        assert_eq!(FrameRef::parse(&frame.evidence()).as_ref(), Some(&frame));
    }

    /// 验收 7：几何缺失时**如实表达"没绑上"**，不得用推测值填充，也不得与"绑错了"混为一谈。
    #[test]
    fn missing_geometry_is_unbindable_not_mismatch_and_never_fabricated() {
        for missing_field in ["image.sha256", "image.width", "image.height", "image.screen_rect"] {
            let mut value = state(DIGEST_A, 2560, 1152);
            match missing_field {
                "image.sha256" => {
                    value["image"].as_object_mut().expect("obj").remove("sha256");
                }
                "image.width" => {
                    value["image"].as_object_mut().expect("obj").remove("width");
                }
                "image.height" => {
                    value["image"].as_object_mut().expect("obj").remove("height");
                }
                _ => {
                    value["image"].as_object_mut().expect("obj").remove("screen_rect");
                }
            }
            let error = FrameRef::bind(&value, &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
                .expect_err("缺字段不得建立绑定");
            assert_eq!(error.code(), "frame_unbindable", "{missing_field}");
            let FrameBindingError::Unbindable(unbindable) = &error else {
                panic!("缺字段必须是 Unbindable，不得报成错配");
            };
            assert!(
                unbindable.missing.contains(&missing_field),
                "必须点名缺了 {missing_field}，实际 {:?}",
                unbindable.missing
            );
        }
        // 完全没有 screenshot 证据时同样不得建立绑定。
        let error = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[], CANVAS)
            .expect_err("无证据串不得建立绑定");
        assert_eq!(error.code(), "frame_unbindable");
    }

    /// 验收 8：映射口径与 `stroke_arguments` 同一份约定（含端点），保证"文档说的就是代码做的"。
    #[test]
    fn unit_point_mapping_matches_the_stroke_convention_including_endpoints() {
        let frame = FrameRef::bind(&state(DIGEST_A, 2560, 1152), &[evidence(DIGEST_A, 2560, 1152)], CANVAS)
            .expect("绑定");
        let rect = frame.canvas_rect;
        // (0,0) 命中容器左上角；(1,1) 命中右下角最后一个像素（size-1，不越界）。
        assert_eq!(frame.map_unit_point(0.0, 0.0), (rect[0], rect[1]));
        assert_eq!(
            frame.map_unit_point(1.0, 1.0),
            (rect[0] + rect[2] - 1, rect[1] + rect[3] - 1)
        );
        // 端点内的归一化点必须始终落在容器内。
        for step in 0..=10 {
            let value = f64::from(step) / 10.0;
            let (x, y) = frame.map_unit_point(value, value);
            assert!(x >= rect[0] && x < rect[0] + rect[2], "x={x} 越界");
            assert!(y >= rect[1] && y < rect[1] + rect[3], "y={y} 越界");
        }
    }
}
