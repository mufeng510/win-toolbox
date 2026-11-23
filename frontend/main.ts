/**
 * win-toolbox 前端入口（原生 TypeScript + Vite，无框架）。
 * 只负责界面与状态；所有数据经由 api.ts 调用 Rust 后端。
 */

import { open, type OpenDialogOptions } from "@tauri-apps/plugin-dialog";
import {
  api,
  type AppConfig,
  type AppInfo,
  type ClassicMenuStatus,
  type FlattenConfig,
  type HashAlgorithm,
  type HashRenameConfig,
  type HeicToJpgConfig,
  type ImageMergeConfig,
  type MenuLocation,
  type MenuStatusItem,
  type ToolInfo,
} from "./api";

/* ------------------------------------------------------------------ */
/* 小工具函数                                                          */
/* ------------------------------------------------------------------ */

/** 获取元素，缺失时尽早报错而不是静默失败。 */
function el(id: string): HTMLElement {
  const node = document.getElementById(id);
  if (!node) throw new Error(`缺少界面节点 #${id}`);
  return node;
}

/** HTML 转义：所有来自后端的字符串在拼模板前都必须经过。 */
function escapeHtml(value: unknown): string {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function platformLabel(platform: string): string {
  const map: Record<string, string> = {
    windows: "Windows",
    macos: "macOS",
    linux: "Linux",
    unknown: "未知平台",
  };
  return map[platform] ?? platform;
}

function locationLabel(location: MenuLocation): string {
  const map: Record<MenuLocation, string> = {
    directory: "文件夹右键",
    directory_background: "文件夹空白处",
    file: "文件右键",
  };
  return map[location] ?? location;
}

function argLabel(arg: string): string {
  return arg === "path_v" ? "%V" : arg === "path1" ? "%1" : arg;
}

/* ------------------------------------------------------------------ */
/* 应用状态                                                            */
/* ------------------------------------------------------------------ */

interface ReportEntry {
  id: number;
  ok: boolean;
  scope: string;
  message: string;
  details: string[];
  time: string;
}

interface AppState {
  appInfo: AppInfo | null;
  tools: ToolInfo[];
  menuItems: MenuStatusItem[];
  classic: ClassicMenuStatus | null;
  config: AppConfig;
  selectedMenuIds: Set<string>;
  targetPaths: Map<string, string>;
  reports: ReportEntry[];
  errors: string[];
}

const state: AppState = {
  appInfo: null,
  tools: [],
  menuItems: [],
  classic: null,
  config: {},
  selectedMenuIds: new Set<string>(),
  targetPaths: new Map<string, string>(),
  reports: [],
  errors: [],
};

/* ------------------------------ 配置读写 ---------------------------- */

function sectionOf(tool: string): Record<string, unknown> {
  const existing = state.config[tool];
  if (existing && typeof existing === "object") {
    return existing as Record<string, unknown>;
  }
  const fresh: Record<string, unknown> = {};
  state.config[tool] = fresh;
  return fresh;
}

function hashConfig(): HashRenameConfig {
  const raw = state.config["hash-rename"] as Partial<HashRenameConfig> | undefined;
  const algo = raw?.algorithm;
  const algorithm: HashAlgorithm =
    algo === "md5" || algo === "sha256" || algo === "xxh3" ? algo : "sha256";
  return { algorithm, preview: raw?.preview ?? true };
}

function heicConfig(): HeicToJpgConfig {
  const raw = state.config["heic-to-jpg"] as Partial<HeicToJpgConfig> | undefined;
  const q = raw?.quality;
  const quality = typeof q === "number" && q >= 1 && q <= 100 ? Math.round(q) : 85;
  return {
    quality,
    recursive: raw?.recursive ?? false,
    delete_original: raw?.delete_original ?? false,
  };
}

function flattenConfig(): FlattenConfig {
  const raw = state.config["folder-flatten"] as Partial<FlattenConfig> | undefined;
  const conflict = raw?.on_conflict;
  const on_conflict: FlattenConfig["on_conflict"] =
    conflict === "auto_rename" || conflict === "skip" ? conflict : "auto_rename";
  return { dry_run: raw?.dry_run ?? false, on_conflict };
}

function imageMergeConfig(): ImageMergeConfig {
  const raw = state.config["image-merge"] as Partial<ImageMergeConfig> | undefined;
  const q = raw?.quality;
  const quality = typeof q === "number" && q >= 1 && q <= 100 ? Math.round(q) : 95;
  const o = raw?.overlap;
  const overlap = typeof o === "number" && Number.isFinite(o) && o >= 0 ? Math.floor(o) : 0;
  return { quality, overlap, delete_sources: raw?.delete_sources ?? true };
}

/** 组装 run_tool 的 options；键名即后端约定的原始键名，不在此做重命名。 */
function toolOptions(toolId: string): Record<string, unknown> {
  if (toolId === "hash-rename") {
    const c = hashConfig();
    return { algorithm: c.algorithm, preview: c.preview };
  }
  if (toolId === "heic-to-jpg") {
    const c = heicConfig();
    return { quality: c.quality, recursive: c.recursive, delete_original: c.delete_original };
  }
  if (toolId === "folder-flatten") {
    const c = flattenConfig();
    return { dry_run: c.dry_run, on_conflict: c.on_conflict };
  }
  if (toolId === "image-merge") {
    const c = imageMergeConfig();
    return { quality: c.quality, overlap: c.overlap, delete_sources: c.delete_sources };
  }
  return {};
}

async function saveConfig(): Promise<void> {
  const res = await api.setConfig(state.config as Record<string, unknown>);
  if (!res.ok) showBanner(`保存设置失败：${res.error}`);
}

/* ------------------------------------------------------------------ */
/* 数据加载                                                            */
/* ------------------------------------------------------------------ */

async function loadAppInfo(): Promise<void> {
  const res = await api.appInfo();
  if (res.ok) state.appInfo = res.data;
  else state.errors.push(`应用信息加载失败：${res.error}`);
}

async function loadTools(): Promise<void> {
  const res = await api.listTools();
  if (res.ok) state.tools = res.data;
  else state.errors.push(`工具列表加载失败：${res.error}`);
}

async function loadMenu(): Promise<void> {
  const res = await api.menuStatus();
  if (res.ok) {
    state.menuItems = res.data;
    // 默认勾选当前已启用的项，语义直观：勾选 = 期望安装。
    state.selectedMenuIds = new Set(
      res.data.filter((item) => item.enabled).map((item) => item.id),
    );
  } else {
    state.errors.push(`右键菜单状态加载失败：${res.error}`);
  }
}

async function loadClassic(): Promise<void> {
  const res = await api.classicMenuStatus();
  if (res.ok) state.classic = res.data;
  else state.errors.push(`经典菜单状态加载失败：${res.error}`);
}

async function loadConfig(): Promise<void> {
  const res = await api.getConfig();
  if (res.ok) state.config = (res.data ?? {}) as AppConfig;
  else state.errors.push(`配置加载失败：${res.error}`);
}

/** 刷新单项状态（操作后调用），不重绘整个应用。 */
async function refreshMenu(): Promise<void> {
  await loadMenu();
  renderMenu();
}

async function refreshClassic(): Promise<void> {
  await loadClassic();
  renderClassic();
}

/* ------------------------------------------------------------------ */
/* 渲染：顶部 / 横幅 / 结果                                            */
/* ------------------------------------------------------------------ */

function renderHeader(): void {
  el("app-version").textContent = state.appInfo?.version
    ? `v${state.appInfo.version}`
    : "版本未知";
  el("app-platform").textContent = state.appInfo?.platform
    ? platformLabel(state.appInfo.platform)
    : "平台未知";
}

function renderBanner(): void {
  const banner = el("banner");
  if (state.errors.length === 0) {
    banner.hidden = true;
    banner.innerHTML = "";
    return;
  }
  banner.hidden = false;
  banner.innerHTML = `
    <div class="banner" role="alert">
      <div class="banner-icon" aria-hidden="true">⚠</div>
      <div class="banner-messages">
        ${state.errors.map((e) => `<div>${escapeHtml(e)}</div>`).join("")}
      </div>
      <button class="banner-close" id="banner-close" aria-label="关闭提示">✕</button>
    </div>`;
  el("banner-close").addEventListener("click", () => {
    state.errors = [];
    renderBanner();
  });
}

function showBanner(message: string): void {
  if (!state.errors.includes(message)) state.errors.push(message);
  renderBanner();
}

function pushReport(
  ok: boolean,
  scope: string,
  message: string,
  details: string[],
): void {
  state.reports.push({
    id: Date.now() + Math.random(),
    ok,
    scope,
    message,
    details: Array.isArray(details) ? details : [],
    time: new Date().toLocaleTimeString("zh-CN", { hour12: false }),
  });
  // 只保留最近 50 条，避免 DOM 无限增长。
  if (state.reports.length > 50) state.reports.shift();
}

function renderReports(): void {
  const list = el("report-list");
  el("report-count").textContent = String(state.reports.length);
  if (state.reports.length === 0) {
    list.innerHTML = `<p class="empty">暂无任务结果。运行工具或安装菜单后会显示在这里。</p>`;
    return;
  }
  list.innerHTML = state.reports
    .slice()
    .reverse()
    .map(
      (r) => `
      <article class="report ${r.ok ? "report-ok" : "report-fail"}">
        <div class="report-head">
          <span class="report-scope">${escapeHtml(r.scope)}</span>
          <time>${escapeHtml(r.time)}</time>
        </div>
        <div class="report-message">${escapeHtml(r.message)}</div>
        ${
          r.details.length
            ? `<ul class="report-details">${r.details
                .map((d) => `<li>${escapeHtml(d)}</li>`)
                .join("")}</ul>`
            : ""
        }
      </article>`,
    )
    .join("");
}

/* ------------------------------------------------------------------ */
/* 渲染：工具卡片                                                      */
/* ------------------------------------------------------------------ */

function settingsMarkup(tool: ToolInfo): string {
  if (tool.id === "hash-rename") {
    const c = hashConfig();
    return `
      <div class="settings-inner">
        <label class="field">
          <span class="field-label">哈希算法</span>
          <select class="input" data-tool="hash-rename" data-setting="algorithm">
            <option value="md5" ${c.algorithm === "md5" ? "selected" : ""}>MD5</option>
            <option value="sha256" ${c.algorithm === "sha256" ? "selected" : ""}>SHA-256</option>
            <option value="xxh3" ${c.algorithm === "xxh3" ? "selected" : ""}>xxH3</option>
          </select>
        </label>
        <label class="check-row">
          <input type="checkbox" data-tool="hash-rename" data-setting="preview" ${c.preview ? "checked" : ""} />
          <span>预览模式（仅列出将要执行的操作，不实际修改文件）</span>
        </label>
      </div>`;
  }
  if (tool.id === "heic-to-jpg") {
    const c = heicConfig();
    return `
      <div class="settings-inner">
        <label class="field">
          <span class="field-label">JPG 质量 <output class="range-value" data-role="quality-value">${c.quality}</output></span>
          <input class="range" type="range" min="1" max="100" step="1"
            data-tool="heic-to-jpg" data-setting="quality" value="${c.quality}" />
        </label>
        <label class="check-row">
          <input type="checkbox" data-tool="heic-to-jpg" data-setting="recursive" ${c.recursive ? "checked" : ""} />
          <span>递归处理子文件夹</span>
        </label>
        <label class="check-row">
          <input type="checkbox" data-tool="heic-to-jpg" data-setting="delete_original" ${c.delete_original ? "checked" : ""} />
          <span>删除原 HEIC 文件（默认保留，请谨慎勾选）</span>
        </label>
      </div>`;
  }
  if (tool.id === "folder-flatten") {
    const c = flattenConfig();
    return `
      <div class="settings-inner">
        <label class="check-row">
          <input type="checkbox" data-tool="folder-flatten" data-setting="dry_run" ${c.dry_run ? "checked" : ""} />
          <span>预览模式（只列出将要执行的操作，不实际修改）</span>
        </label>
        <label class="field">
          <span class="field-label">重名处理</span>
          <select class="input" data-tool="folder-flatten" data-setting="on_conflict">
            <option value="auto_rename" ${c.on_conflict === "auto_rename" ? "selected" : ""}>自动重命名（加序号，不覆盖）</option>
            <option value="skip" ${c.on_conflict === "skip" ? "selected" : ""}>跳过重名文件</option>
          </select>
        </label>
      </div>`;
  }
  if (tool.id === "image-merge") {
    const c = imageMergeConfig();
    return `
      <div class="settings-inner">
        <label class="field">
          <span class="field-label">JPG 质量 <output class="range-value" data-role="quality-value">${c.quality}</output></span>
          <input class="range" type="range" min="1" max="100" step="1"
            data-tool="image-merge" data-setting="quality" value="${c.quality}" />
          <span class="muted">仅 JPG 输出时生效。</span>
        </label>
        <label class="field">
          <span class="field-label">重叠像素</span>
          <input class="input" type="number" min="0" step="1"
            data-tool="image-merge" data-setting="overlap" value="${c.overlap}" />
          <span class="muted">切割的图片若有重叠像素才填写，默认 0。</span>
        </label>
        <label class="check-row">
          <input type="checkbox" data-tool="image-merge" data-setting="delete_sources" ${c.delete_sources ? "checked" : ""} />
          <span>合并成功后删除原图，不可恢复（默认勾选）</span>
        </label>
      </div>`;
  }
  return `<p class="muted settings-note">此工具在下方「系统设置 · 经典右键菜单」区操作。</p>`;
}

/** 运行按钮文案：图片合并走多选对话框，按钮直接说明动作。 */
function runButtonLabel(toolId: string): string {
  return toolId === "image-merge" ? "选择图片并合并" : "运行";
}

function renderTools(): void {
  const grid = el("tools-grid");
  // 系统设置类工具有独立的操作区（下方的经典菜单面板），不在工具列表重复展示。
  const visible = state.tools.filter((t) => !t.capabilities?.system_tweak);
  if (visible.length === 0) {
    grid.innerHTML = `<p class="empty">未获取到工具列表。</p>`;
    return;
  }
  grid.innerHTML = visible
    .map((tool) => {
      const platformBadges = (tool.platforms ?? [])
        .map((p) => `<span class="badge">${escapeHtml(platformLabel(p))}</span>`)
        .join("");
      const capBadges: string[] = [];
      if (tool.capabilities?.context_menu?.length) {
        capBadges.push(
          `<span class="badge badge-purple">右键菜单 · ${tool.capabilities.context_menu.length}</span>`,
        );
      }
      if (tool.capabilities?.system_tweak) {
        capBadges.push(`<span class="badge badge-orange">系统设置</span>`);
      }
      const targetValue = escapeHtml(state.targetPaths.get(tool.id) ?? "");
      // 图片合并的图片路径来自多选对话框，不显示目标路径输入框与「浏览」按钮。
      const isImageMerge = tool.id === "image-merge";
      const targetMarkup = isImageMerge
        ? ""
        : `
          <div class="card-target">
            <label class="field-label" for="target-${escapeHtml(tool.id)}">目标路径（可选）</label>
            <div class="input-row">
              <input id="target-${escapeHtml(tool.id)}" class="input" type="text"
                data-role="target" value="${targetValue}"
                placeholder="选择或输入文件夹，留空由后端提示" />
              <button class="btn btn-ghost" data-action="browse" title="浏览文件夹">浏览</button>
            </div>
          </div>`;
      return `
        <article class="card" data-tool-id="${escapeHtml(tool.id)}">
          <div class="card-head">
            <h3>${escapeHtml(tool.name)}</h3>
            <div class="badges">${platformBadges}${capBadges.join("")}</div>
          </div>
          <p class="card-desc">${escapeHtml(tool.description)}</p>
          ${targetMarkup}
          <div class="card-actions">
            <button class="btn btn-primary" data-action="run">${runButtonLabel(tool.id)}</button>
            <button class="btn" data-action="toggle-settings" aria-expanded="false">设置</button>
          </div>
          <div class="settings" data-role="settings" hidden>${settingsMarkup(tool)}</div>
        </article>`;
    })
    .join("");
}

async function onBrowse(card: HTMLElement, toolId: string): Promise<void> {
  const input = card.querySelector('[data-role="target"]') as HTMLInputElement | null;
  try {
    const selected = await open({ directory: true, multiple: false, title: "选择文件夹" });
    if (typeof selected === "string" && selected) {
      state.targetPaths.set(toolId, selected);
      if (input) input.value = selected;
    }
  } catch {
    showBanner("文件选择对话框不可用，请手动输入路径（需在 Tauri 中注册 plugin-dialog）");
  }
}

async function onRun(card: HTMLElement, toolId: string, button: HTMLButtonElement): Promise<void> {
  const tool = state.tools.find((t) => t.id === toolId);
  const scope = tool?.name ?? toolId;

  // 图片合并的图片路径来自多选对话框，不走「目标路径」输入框。
  if (toolId === "image-merge") {
    await runImageMerge(scope, button);
    return;
  }

  const input = card.querySelector('[data-role="target"]') as HTMLInputElement | null;
  const target = input?.value.trim() ? input.value.trim() : null;

  button.disabled = true;
  button.textContent = "运行中…";
  try {
    const res = await api.runTool(toolId, target, toolOptions(toolId));
    if (!res.ok) {
      pushReport(false, scope, `调用失败：${res.error}`, []);
    } else {
      pushReport(res.data.ok, scope, res.data.message, res.data.details);
    }
  } finally {
    button.disabled = false;
    button.textContent = runButtonLabel(toolId);
    renderReports();
  }
}

/** 图片合并：多选图片 → 数量校验 → 自定义确认弹窗 → 调用后端。 */
async function runImageMerge(scope: string, button: HTMLButtonElement): Promise<void> {
  // 标注字面量类型以命中 open() 的「多选文件」重载，返回值稳定为 string[] | null。
  const dialogOptions: OpenDialogOptions & { directory: false; multiple: true } = {
    multiple: true,
    directory: false,
    filters: [{ name: "图片", extensions: ["jpg", "jpeg", "png", "bmp", "webp"] }],
    title: "选择要合并的图片（按文件名排序，两两一组）",
  };

  let paths: string[];
  try {
    const selected = await open(dialogOptions);
    if (selected === null) return; // 用户取消，静默返回
    paths = selected;
  } catch {
    showBanner("文件选择对话框不可用，请检查 plugin-dialog 配置");
    return;
  }

  if (paths.length < 2) {
    showBanner("请至少选择 2 张图片后再合并（图片按文件名排序，两两一组）。");
    return;
  }

  const confirmed = await confirmDialog({
    title: "确认合并图片",
    bodyHtml:
      "将按文件名排序<b>两两合并</b>；输出到第一张图片所在文件夹；合并成功并验证输出有效后<b>会删除原图，此操作不可恢复</b>。",
    confirmText: "确认合并",
    danger: true,
  });
  if (!confirmed) return;

  button.disabled = true;
  button.textContent = "运行中…";
  try {
    const res = await api.runTool("image-merge", paths[0], {
      ...toolOptions("image-merge"),
      paths,
    });
    if (!res.ok) {
      pushReport(false, scope, `调用失败：${res.error}`, []);
    } else {
      pushReport(res.data.ok, scope, res.data.message, res.data.details);
    }
  } finally {
    button.disabled = false;
    button.textContent = runButtonLabel("image-merge");
    renderReports();
  }
}

function bindToolsEvents(): void {
  const grid = el("tools-grid");

  grid.addEventListener("click", (event) => {
    const btn = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
    if (!btn) return;
    const card = btn.closest<HTMLElement>(".card");
    if (!card) return;
    const toolId = card.dataset.toolId ?? "";
    const action = btn.dataset.action;

    if (action === "toggle-settings") {
      const settings = card.querySelector<HTMLElement>('[data-role="settings"]');
      if (!settings) return;
      const expanded = btn.getAttribute("aria-expanded") === "true";
      btn.setAttribute("aria-expanded", String(!expanded));
      btn.textContent = expanded ? "设置" : "收起设置";
      settings.hidden = expanded;
      return;
    }
    if (action === "browse") {
      void onBrowse(card, toolId);
      return;
    }
    if (action === "run") {
      void onRun(card, toolId, btn);
    }
  });

  grid.addEventListener("change", (event) => {
    const node = event.target as HTMLInputElement | HTMLSelectElement;

    // 目标路径输入
    if (node.dataset.role === "target") {
      const card = node.closest<HTMLElement>(".card");
      if (card?.dataset.toolId) {
        state.targetPaths.set(card.dataset.toolId, (node as HTMLInputElement).value);
      }
      return;
    }

    // 设置项
    const tool = node.dataset.tool;
    const setting = node.dataset.setting;
    if (!tool || !setting) return;
    const section = sectionOf(tool);
    if (node instanceof HTMLInputElement && node.type === "checkbox") {
      section[setting] = node.checked;
    } else if (node instanceof HTMLSelectElement) {
      section[setting] = node.value;
    } else if (node instanceof HTMLInputElement && node.type === "range") {
      section[setting] = Number(node.value);
    } else if (node instanceof HTMLInputElement && node.type === "number") {
      const value = Number(node.value);
      section[setting] = Number.isFinite(value) && value >= 0 ? Math.floor(value) : 0;
    }
    void saveConfig();
  });

  grid.addEventListener("input", (event) => {
    const node = event.target as HTMLInputElement;
    if (node instanceof HTMLInputElement && node.type === "range" && node.dataset.setting === "quality") {
      const card = node.closest<HTMLElement>(".card");
      const output = card?.querySelector<HTMLOutputElement>('[data-role="quality-value"]');
      if (output) output.textContent = node.value;
    }
  });
}

/* ------------------------------------------------------------------ */
/* 渲染：右键菜单管理                                                  */
/* ------------------------------------------------------------------ */

function renderMenu(): void {
  const container = el("menu-list");
  if (state.menuItems.length === 0) {
    container.innerHTML = `<p class="empty">没有可注册的右键菜单项。</p>`;
    return;
  }

  const groups = new Map<string, MenuStatusItem[]>();
  for (const item of state.menuItems) {
    const key = item.tool_id ?? "未知工具";
    const list = groups.get(key) ?? [];
    list.push(item);
    groups.set(key, list);
  }

  container.innerHTML = Array.from(groups.entries())
    .map(([toolId, items]) => {
      const title = items[0]?.tool_name || toolId;
      return `
        <div class="menu-group">
          <div class="menu-group-title">${escapeHtml(title)}</div>
          ${items
            .map(
              (item) => `
            <label class="menu-row">
              <input type="checkbox" data-menu-id="${escapeHtml(item.id)}"
                ${state.selectedMenuIds.has(item.id) ? "checked" : ""} />
              <span class="menu-main">
                <span class="menu-label">${escapeHtml(item.label)}</span>
                <span class="badge">${escapeHtml(locationLabel(item.location))}</span>
                <span class="badge badge-subtle">${escapeHtml(argLabel(item.arg))}</span>
              </span>
              <span class="status ${item.enabled ? "status-on" : "status-off"}">
                ${item.enabled ? "已启用" : "未启用"}
              </span>
            </label>`,
            )
            .join("")}
        </div>`;
    })
    .join("");
}

type MenuAction = "install" | "uninstall" | "install-all" | "uninstall-all";

async function handleMenuAction(action: MenuAction): Promise<void> {
  let ids: string[];
  if (action === "install-all" || action === "uninstall-all") {
    ids = state.menuItems.map((item) => item.id);
    state.selectedMenuIds = new Set(ids);
    renderMenu();
  } else {
    ids = Array.from(state.selectedMenuIds);
  }

  if (ids.length === 0) {
    showBanner("请先勾选至少一个菜单项。");
    return;
  }

  const isInstall = action === "install" || action === "install-all";
  const res = isInstall
    ? await api.installMenus(ids)
    : await api.uninstallMenus(ids);

  if (!res.ok) {
    pushReport(false, "右键菜单", `操作失败：${res.error}`, []);
    renderReports();
    return;
  }
  pushReport(
    true,
    "右键菜单",
    isInstall ? `已安装 ${ids.length} 个菜单项` : `已卸载 ${ids.length} 个菜单项`,
    res.data,
  );
  await refreshMenu();
  renderReports();
}

function bindMenuEvents(): void {
  el("menu-list").addEventListener("change", (event) => {
    const node = event.target as HTMLInputElement;
    const id = node.dataset.menuId;
    if (!id) return;
    if (node.checked) state.selectedMenuIds.add(id);
    else state.selectedMenuIds.delete(id);
  });

  el("menu-install").addEventListener("click", () => void handleMenuAction("install"));
  el("menu-uninstall").addEventListener("click", () => void handleMenuAction("uninstall"));
  el("menu-install-all").addEventListener("click", () => void handleMenuAction("install-all"));
  el("menu-uninstall-all").addEventListener("click", () => void handleMenuAction("uninstall-all"));
}

/* ------------------------------------------------------------------ */
/* 自定义确认弹窗（不使用 confirm()）                                  */
/* ------------------------------------------------------------------ */

interface ConfirmOptions {
  title: string;
  /** 允许富文本（调用方需自行保证动态内容已转义）。 */
  bodyHtml: string;
  confirmText: string;
  danger?: boolean;
}

function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    const root = el("modal-root");
    const previousFocus = document.activeElement as HTMLElement | null;

    root.innerHTML = `
      <div class="modal-overlay">
        <div class="modal" role="dialog" aria-modal="true" aria-labelledby="modal-title" aria-describedby="modal-body">
          <h3 id="modal-title">${escapeHtml(options.title)}</h3>
          <div id="modal-body" class="modal-body">${options.bodyHtml}</div>
          <div class="modal-actions">
            <button class="btn" data-modal="cancel">取消</button>
            <button class="btn ${options.danger ? "btn-danger" : "btn-primary"}" data-modal="confirm">
              ${escapeHtml(options.confirmText)}
            </button>
          </div>
        </div>
      </div>`;

    const overlay = root.querySelector<HTMLElement>(".modal-overlay");
    const cancelBtn = root.querySelector<HTMLButtonElement>('[data-modal="cancel"]');
    const confirmBtn = root.querySelector<HTMLButtonElement>('[data-modal="confirm"]');
    if (!overlay || !cancelBtn || !confirmBtn) {
      root.innerHTML = "";
      resolve(false);
      return;
    }

    let settled = false;
    const finish = (value: boolean): void => {
      if (settled) return;
      settled = true;
      document.removeEventListener("keydown", onKeyDown);
      root.innerHTML = "";
      previousFocus?.focus();
      resolve(value);
    };

    const focusables = [cancelBtn, confirmBtn];
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        finish(false);
      } else if (event.key === "Tab") {
        // 简易焦点陷阱：只在两个按钮间循环。
        const index = focusables.indexOf(document.activeElement as HTMLButtonElement);
        if (event.shiftKey && index <= 0) {
          event.preventDefault();
          focusables[focusables.length - 1].focus();
        } else if (!event.shiftKey && index === focusables.length - 1) {
          event.preventDefault();
          focusables[0].focus();
        }
      }
    };

    document.addEventListener("keydown", onKeyDown);
    cancelBtn.addEventListener("click", () => finish(false));
    confirmBtn.addEventListener("click", () => finish(true));
    overlay.addEventListener("click", (event) => {
      if (event.target === overlay) finish(false);
    });
    confirmBtn.focus();
  });
}

/* ------------------------------------------------------------------ */
/* 渲染 / 交互：系统设置（经典菜单）                                   */
/* ------------------------------------------------------------------ */

function renderClassic(): void {
  const body = el("classic-body");
  const status = state.classic;
  if (!status) {
    body.innerHTML = `<p class="empty">未能获取经典菜单状态。</p>`;
    return;
  }
  const supportBadge = status.supported
    ? `<span class="badge badge-green">当前系统支持</span>`
    : `<span class="badge badge-red">当前系统不支持</span>`;
  const stateBadge = status.enabled
    ? `<span class="badge badge-green">经典菜单已启用</span>`
    : `<span class="badge">当前为新菜单</span>`;

  body.innerHTML = `
    <div class="classic-status">${supportBadge}${stateBadge}</div>
    <p class="muted">${escapeHtml(status.detail)}</p>
    <div class="card-actions">
      <button class="btn btn-primary" data-classic="on" ${status.supported ? "" : "disabled"}>启用经典菜单</button>
      <button class="btn" data-classic="off" ${status.supported ? "" : "disabled"}>恢复新菜单</button>
    </div>`;
}

function bindClassicEvents(): void {
  el("classic-body").addEventListener("click", (event) => {
    const btn = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-classic]");
    if (!btn) return;
    const enable = btn.dataset.classic === "on";

    const title = enable ? "启用经典右键菜单？" : "恢复 Windows 11 新菜单？";
    const bodyHtml = enable
      ? `此操作会修改注册表，把<b>当前用户（HKCU）</b>的右键菜单切换为经典样式。<br>
         <b>影响：</b>需要重启资源管理器后生效；无需管理员权限；不会改动其他用户。<br>
         <b>可逆性：</b>随时可点击「恢复新菜单」一键还原。`
      : `此操作会把右键菜单恢复为 Windows 11 新样式。<br>
         <b>影响：</b>只修改当前用户（HKCU）的注册表；无需管理员权限；需要重启资源管理器生效。<br>
         <b>可逆性：</b>随时可再次点击「启用经典菜单」还原。`;

    void (async () => {
      const confirmed = await confirmDialog({
        title,
        bodyHtml,
        confirmText: enable ? "确认启用" : "确认恢复",
        danger: true,
      });
      if (!confirmed) return;

      btn.disabled = true;
      try {
        const res = await api.classicMenuSet(enable);
        if (!res.ok) {
          pushReport(false, "经典菜单", `操作失败：${res.error}`, []);
        } else {
          pushReport(res.data.ok, "经典菜单", res.data.message, res.data.details);
        }
      } finally {
        await refreshClassic();
        renderReports();
      }
    })();
  });
}

/* ------------------------------------------------------------------ */
/* 启动                                                                */
/* ------------------------------------------------------------------ */

function renderAll(): void {
  renderHeader();
  renderTools();
  renderMenu();
  renderClassic();
  renderReports();
  renderBanner();
}

async function init(): Promise<void> {
  bindToolsEvents();
  bindMenuEvents();
  bindClassicEvents();
  el("report-clear").addEventListener("click", () => {
    state.reports = [];
    renderReports();
  });

  // 并行加载所有数据；单个失败不会阻塞其他区块。
  await Promise.all([
    loadAppInfo(),
    loadTools(),
    loadMenu(),
    loadClassic(),
    loadConfig(),
  ]);
  renderAll();
}

void init();
