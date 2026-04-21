import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type ComponentSlug = 'nginx' | 'php' | 'mariadb' | 'phpmyadmin';

export interface ComponentInfo {
  slug: ComponentSlug;
  name: string;
}

export type ServiceStatus =
  | 'stopped'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'crashed';

export interface ServiceHandleDto {
  slug: ComponentSlug;
  pid: number;
}

export interface ServiceStatusEvent {
  slug: ComponentSlug;
  status: ServiceStatus;
}

/** Subscribe to `service-status` events emitted by the backend watcher. */
export function onServiceStatus(
  cb: (event: ServiceStatusEvent) => void,
): Promise<UnlistenFn> {
  return listen<ServiceStatusEvent>('service-status', (e) => cb(e.payload));
}

// --- Logs ------------------------------------------------------------------

export type LogStream = 'stdout' | 'stderr';

export interface LogLine {
  seq: number;
  ts_ms: number;
  stream: LogStream;
  text: string;
}

export interface LogLineEvent {
  slug: ComponentSlug;
  line: LogLine;
}

/** Subscribe to per-line events emitted while a service is running. */
export function onServiceLog(
  cb: (event: LogLineEvent) => void,
): Promise<UnlistenFn> {
  return listen<LogLineEvent>('service-log', (e) => cb(e.payload));
}

// --- Config ----------------------------------------------------------------

export type Language = 'pt-br' | 'en';

export interface PortConfig {
  http: number;
  mariadb: number;
  php_fcgi: number;
  bind_address: string; // "127.0.0.1" or "0.0.0.0"
}

export interface Prefs {
  language: Language;
  open_browser_on_start: boolean;
  minimize_to_tray_on_start: boolean;
}

export interface AppConfigDto {
  ports: PortConfig;
  prefs: Prefs;
}

export interface PortOccupier {
  pid: number;
  process_name: string | null;
  exe_path: string | null;
}

export interface PortInspection {
  free: boolean;
  occupier: PortOccupier | null;
  /// True when the occupier is a MadiStack-managed service (our own nginx,
  /// php-cgi, mysqld running). The UI should treat this as informational,
  /// not as a conflict.
  is_self: boolean;
}

export const ipc = {
  ping: () => invoke<string>('ping'),
  listComponents: () => invoke<ComponentInfo[]>('list_components'),
  portAvailable: (port: number) => invoke<boolean>('port_available', { port }),
  portInspect: (port: number) => invoke<PortInspection>('port_inspect', { port }),
  serviceStart: (component: ComponentSlug) =>
    invoke<ServiceHandleDto>('service_start', { component }),
  serviceStop: (component: ComponentSlug) =>
    invoke<void>('service_stop', { component }),
  serviceStatus: (component: ComponentSlug) =>
    invoke<ServiceStatus>('service_status', { component }),
  serviceLogs: (component: ComponentSlug, since = 0) =>
    invoke<LogLine[]>('service_logs', { component, since }),
  getConfig: () => invoke<AppConfigDto>('get_config'),
  saveConfig: (config: AppConfigDto) =>
    invoke<void>('save_config', { config }),
  firewallEnsureRules: () => invoke<void>('firewall_ensure_rules'),
  firewallRemoveRules: () => invoke<void>('firewall_remove_rules'),
  firewallRulesStatus: () => invoke<FirewallRulesStatus>('firewall_rules_status'),
  componentInstalled: (component: ComponentSlug) =>
    invoke<boolean>('component_installed', { component }),
  componentInstall: (component: ComponentSlug) =>
    invoke<void>('component_install', { component }),
  installAll: () => invoke<void>('install_all'),
  updaterCheck: () => invoke<UpdateStatusDto[]>('updater_check'),
  updaterApply: (component: ComponentSlug) =>
    invoke<string>('updater_apply', { component }),
  updaterRollback: (component: ComponentSlug) =>
    invoke<void>('updater_rollback', { component }),
  vhostList: () => invoke<VhostDto[]>('vhost_list'),
  vhostEnable: (name: string) => invoke<void>('vhost_enable', { name }),
  vhostDisable: (name: string) => invoke<void>('vhost_disable', { name }),
};

export interface VhostDto {
  name: string;
  hostname: string;
  enabled: boolean;
}

export interface FirewallRulesStatus {
  nginx: boolean;
  mariadb: boolean;
  php_fcgi: boolean;
}

// --- Install progress ------------------------------------------------------

export type InstallPhase =
  | 'resolving'
  | 'downloading'
  | 'verifying'
  | 'extracting'
  | 'done'
  | 'error';

export interface InstallProgressEvent {
  slug: ComponentSlug;
  phase: InstallPhase;
  bytes?: number;
  total?: number;
  message?: string;
}

export function onInstallProgress(
  cb: (event: InstallProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<InstallProgressEvent>('install-progress', (e) => cb(e.payload));
}

// --- Updater ---------------------------------------------------------------

export interface UpdateStatusDto {
  slug: ComponentSlug;
  current: string | null;
  available: string;
  update_available: boolean;
  /// True when the component's signature binary exists on disk. Lets the UI
  /// distinguish "instalado sem versão registrada" from "não instalado".
  installed_on_disk: boolean;
}

export type UpdatePhase =
  | 'downloading'
  | 'verifying'
  | 'extracting'
  | 'done'
  | 'error';

export interface UpdateProgressEvent {
  slug: ComponentSlug;
  phase: UpdatePhase;
  bytes?: number;
  total?: number;
  message?: string;
}

export function onUpdateProgress(
  cb: (event: UpdateProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<UpdateProgressEvent>('update-progress', (e) => cb(e.payload));
}
