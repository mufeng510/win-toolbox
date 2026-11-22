/**
 * 后端 invoke 封装与类型定义。
 *
 * TS 侧类型严格对齐 Rust（crates/toolbox-core/src/tool.rs）的 serde 形状：
 * - MenuLocation / MenuArg 使用 #[serde(rename_all = "snake_case")]
 * - ToolReport { ok, message, details }
 *
 * 所有调用统一返回 ApiResult<T>，不在 UI 层抛出原始异常，便于集中做中文错误提示。
 */

import { invoke } from "@tauri-apps/api/core";

/* ------------------------------------------------------------------ */
/* 类型定义（与后端返回对齐）                                          */
/* ------------------------------------------------------------------ */

/** 右键菜单注册位置：directory | directory_background | file */
export type MenuLocation = "directory" | "directory_background" | "file";

/** 传给 exe 的路径参数形式：%V | %1 */
export type MenuArg = "path_v" | "path1";

export interface MenuVerb {
  location: MenuLocation;
  label: string;
  arg: MenuArg;
}

export interface Capabilities {
  context_menu: MenuVerb[];
  system_tweak: boolean;
}

/** list_tools() -> ToolInfo[] */
export interface ToolInfo {
  id: string;
  name: string;
  description: string;
  platforms: string[];
  capabilities: Capabilities;
}

/**
 * menu_status() -> MenuStatusItem[]
 * 注意：后端需返回稳定的 id，前端会把该 id 原样回传给 install_menus/uninstall_menus。
 */
export interface MenuStatusItem {
  id: string;
  tool_id: string;
  tool_name?: string;
  location: MenuLocation;
  label: string;
  arg: MenuArg;
  enabled: boolean;
}

/** run_tool() / classic_menu_set() -> ToolReport */
export interface ToolReport {
  ok: boolean;
  message: string;
  details: string[];
}

/** classic_menu_status() -> ClassicMenuStatus */
export interface ClassicMenuStatus {
  enabled: boolean;
  supported: boolean;
  detail: string;
}

/** app_info() -> AppInfo */
export interface AppInfo {
  version: string;
  platform: string;
}

/* ---------------------------- 配置相关类型 ---------------------------- */

export type HashAlgorithm = "md5" | "sha256" | "xxh3";

export interface HashRenameConfig {
  algorithm: HashAlgorithm;
  preview: boolean;
}

export interface HeicToJpgConfig {
  quality: number;
  recursive: boolean;
  delete_original: boolean;
}

/** 文件夹扁平化：重名策略与预览开关。 */
export interface FlattenConfig {
  dry_run: boolean;
  on_conflict: "auto_rename" | "skip";
}

/** 图片合并：JPG 质量、切割重叠像素、是否删除原图。 */
export interface ImageMergeConfig {
  quality: number;
  overlap: number;
  delete_sources: boolean;
}

/**
 * get_config() / set_config() 的顶层结构，按工具 id 分节
 * （与 Rust config::tool_section / set_tool_section 一致）。
 */
export interface AppConfig {
  "hash-rename"?: Partial<HashRenameConfig> & Record<string, unknown>;
  "heic-to-jpg"?: Partial<HeicToJpgConfig> & Record<string, unknown>;
  "folder-flatten"?: Partial<FlattenConfig> & Record<string, unknown>;
  "image-merge"?: Partial<ImageMergeConfig> & Record<string, unknown>;
  [key: string]: unknown;
}

/* ------------------------------------------------------------------ */
/* invoke 封装                                                         */
/* ------------------------------------------------------------------ */

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

/** 把任意后端错误转换成可读的中文提示。 */
export function normalizeError(err: unknown): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string" && err.trim()) return err;
  if (err && typeof err === "object") {
    const obj = err as { message?: unknown; error?: unknown };
    if (typeof obj.message === "string" && obj.message) return obj.message;
    if (typeof obj.error === "string" && obj.error) return obj.error;
  }
  return "后端返回未知错误，请查看日志";
}

/** 统一的 invoke 调用入口：try/catch + 错误文案转换。 */
export async function call<T>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<ApiResult<T>> {
  try {
    const data = await invoke<T>(cmd, args);
    return { ok: true, data };
  } catch (err) {
    return { ok: false, error: normalizeError(err) };
  }
}

/** 后端命令集合（参数名与 Rust #[tauri::command] 函数签名一一对应）。 */
export const api = {
  /** app_info() -> { version, platform } */
  appInfo: () => call<AppInfo>("app_info"),

  /** list_tools() -> ToolInfo[] */
  listTools: () => call<ToolInfo[]>("list_tools"),

  /** menu_status() -> MenuStatusItem[] */
  menuStatus: () => call<MenuStatusItem[]>("menu_status"),

  /** install_menus(ids: string[]) -> string[] */
  installMenus: (ids: string[]) => call<string[]>("install_menus", { ids }),

  /** uninstall_menus(ids: string[]) -> string[] */
  uninstallMenus: (ids: string[]) => call<string[]>("uninstall_menus", { ids }),

  /** run_tool(id, target, options) -> ToolReport */
  runTool: (id: string, target: string | null, options: Record<string, unknown>) =>
    call<ToolReport>("run_tool", { id, target, options }),

  /** classic_menu_status() -> ClassicMenuStatus */
  classicMenuStatus: () => call<ClassicMenuStatus>("classic_menu_status"),

  /** classic_menu_set(enabled) -> ToolReport */
  classicMenuSet: (enabled: boolean) =>
    call<ToolReport>("classic_menu_set", { enabled }),

  /** get_config() -> Record<string, unknown> */
  getConfig: () => call<Record<string, unknown>>("get_config"),

  /** set_config(config) -> void */
  setConfig: (config: Record<string, unknown>) =>
    call<void>("set_config", { config }),
};
