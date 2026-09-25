# Vision 对外接口说明

## 模块职责

`vision` 负责本地 VLM、远端 VLM、多模态请求、ShowUI grounding 和视觉坐标解析。它只返回识别结果，不直接执行鼠标或键盘操作。

## 对外 crate 与命令

- `coolzhu-vision-service`，兼容 crate alias：`vision`
- `coolzhu-vision-smoke`
- `coolzhu-latest-desktop-vision`

## 稳定接口

- `vision::VisionBackend`
- `vision::VisionRequest`
- `vision::VisionResponse`
- `vision::LocalOpenAiVisionBackend`
- `vision::ZhipuVisionBackend`
- `vision::build_showui_grounding_request`
- `vision::parse_relative_point`
- `vision::relative_point_to_pixel`
- `vision::default_local_vlm_install_root`
- `vision::local_vlm_health_url`
- `vision::local_vlm_resource_launcher_hint`

## 本地 VLM 资源

项目内提供本地 VLM 启动/检查资源：

- `modules/vision/resources/local-vlm/start-local-vlm.ps1`
- `modules/vision/resources/local-vlm/check-local-vlm.ps1`
- `modules/vision/resources/local-vlm/coolzhu-local-vlm.template.json`

默认复用 `%USERPROFILE%\.claw\local-vlm` 下已有的虚拟环境、模型和启动器；可通过 `COOLZHU_LOCAL_VLM_ROOT` 指向其他安装根目录。资源脚本只纳入 launcher/checker 和配置模板，不把 `.venv`、模型权重、下载缓存写入项目仓库。

## 接口变更审查点

- 坐标默认必须保持 `[0, 1]` 相对坐标。
- 本地模型服务接口保持 OpenAI-compatible。
- 删除远端视觉前必须保证本地视觉 smoke test 可用。

## 独立验证

```powershell
cargo check -p coolzhu-vision-service --offline
cargo test -p coolzhu-vision-service --offline
```
