---
name: 缺陷 / Bug
about: 报告一个可复现的问题
title: "<模块前缀> 现象一句话"
labels: "bug"
---

> 正文用中文。

## 现象

看到了什么。贴真实输出/截图/日志，不要只写"不工作"。

## 复现步骤

1. 起什么环境（版本、workspace、是否独立 USERPROFILE）
2. 执行什么（贴可直接跑的命令 / HTTP 请求）
3. 观察到什么

```
# 贴命令与真实输出
```

## 期望

应该发生什么，依据是什么（设计文档 / AC / 既有行为）。

## 环境

- 版本 / commit：
- schema `user_version`：
- 工作区：`~/coolzhuagent`（生产）还是隔离目录
- OS / 编码：Windows 11 中文（注意 GBK/UTF-8 解码问题）

## 证据

日志片段、`goal_events` 事件流、数据库行、网络响应。
定位到具体文件与行号更好（`main.rs:12345`）。
