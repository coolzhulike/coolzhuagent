/* 品牌头像的唯一解析入口；仅迁移明确列举的系统头像，不写入会话存储。 */
(function (root) {
  "use strict";
  const base = "assets/avatars/brand-v1/";
  const pathFor = (brand) => `${base}${brand}.png`;
  const fallback = pathFor("generic");
  const oldFiles = [
    "wuxia-swordsman.png", "robot-gold.png", "robot-cyan.png", "robot-green.png",
    "robot-purple.png", "robot-red.png", "mushroom-butler.png", "star-sprite.png",
    "turtle-mechanic.png", "cat-black.png", "shiba-captain.png", "astronaut-pixel.png",
    "alien-jelly.png", "ai-core.png", "wuxia-v3/bamboo-swordsman.png",
    "wuxia-v3/feather-guard.png", "wuxia-v3/jade-strategist.png",
    "wuxia-v3/gold-commander.png", "wuxia-v3/ink-shadow.png",
    "wuxia-v3/wandering-physician.png", "wuxia-v3/mechanist.png",
    "wuxia-v3/scripture-monk.png",
  ];
  const legacy = new Set(oldFiles.map((file) => `assets/avatars/${file}`));
  const normalize = (value) => String(value || "").trim().replace(/\\/g, "/").replace(/^\.\//, "");
  // 聚合平台的模型名比 provider 更准确：Devin、百炼、OpenRouter 等均可运行其它品牌模型。
  const modelRules = [
    [/(?:^|[\s/:._-])deepseek(?:$|[\s:._-]|\d)/i, "deepseek"],
    [/(?:^|[\s/:._-])(?:chatgpt|gpt(?:$|[\s:._-]|\d)|o[134](?:$|[\s:._-]))/i, "chatgpt"],
    [/(?:^|[\s/:._-])(?:qwen|qwq|qvq)(?:$|[\s:._-]|\d)|千问|通义/i, "qwen"],
    [/(?:^|[\s/:._-])claude(?:$|[\s:._-]|\d)/i, "claude"],
    [/(?:^|[\s/:._-])gemini(?:$|[\s:._-]|\d)/i, "gemini"],
    [/(?:^|[\s/:._-])grok(?:$|[\s:._-]|\d)/i, "grok"],
    [/(?:^|[\s/:._-])glm(?:$|[\s:._-]|\d)|智谱/i, "glm"],
  ];
  const providerRules = [
    [/^(?:deepseek|深度求索)$/i, "deepseek"],
    [/^(?:openai|chatgpt|openai-subscription|chatgpt-subscription)$/i, "chatgpt"],
    [/^(?:qwen|alibaba-bailian|阿里百炼|Alibaba DashScope|通义千问|千问)$/i, "qwen"],
    [/^(?:anthropic|claude|clawapi|Anthropic \/ ClawAPI)$/i, "claude"],
    [/^(?:google|gemini|google-ai)$/i, "gemini"],
    [/^(?:xai|xAI \(Grok\)|grok)$/i, "grok"],
    [/^(?:zhipuai|智谱 AI|智谱 AI \(Z.AI\)|智谱|Zhipu AI|z\.ai)$/i, "glm"],
  ];
  function brandForSession(session) {
    const model = String(session?.model || session?.model_id || "").trim();
    for (const [pattern, brand] of modelRules) if (pattern.test(model)) return brand;
    // 非空、未识别的模型不可被聚合平台的品牌错误覆盖。
    if (model && !/^(?:custom-model|default|auto)$/i.test(model)) return "generic";
    const provider = String(session?.provider || session?.provider_id || "").trim();
    for (const [pattern, brand] of providerRules) if (pattern.test(provider)) return brand;
    return "generic";
  }
  function resolve(session) {
    const custom = normalize(session?.avatar);
    if (custom && !legacy.has(custom)) return custom;
    return pathFor(brandForSession(session));
  }
  function migrate(path, session) {
    const value = normalize(path);
    return legacy.has(value) ? pathFor(brandForSession(session)) : value;
  }
  function url(path) {
    const value = String(path || "");
    // 自定义上传 data/blob/绝对地址保持原值；现有后端权限和路径校验仍由宿主负责。
    return /^(?:data:|blob:|https?:\/\/|\/)/i.test(value) ? value : `./${normalize(value) || fallback}`;
  }
  const api = Object.freeze({ fallback, resolve, migrate, url, brandForSession,
    isLegacy: (path) => legacy.has(normalize(path)),
    legacyPaths: Object.freeze(Array.from(legacy)),
    isPreset: (path) => /^assets\/avatars\/brand-v1\/(?:generic|deepseek|chatgpt|qwen|claude|gemini|grok|glm)\.png$/.test(normalize(path)),
  });
  root.CoolzhuBrandAvatars = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof globalThis !== "undefined" ? globalThis : window);
