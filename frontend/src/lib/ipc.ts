import { invoke } from '@tauri-apps/api/core';

export interface ComponentInfo {
  slug: 'nginx' | 'php' | 'mariadb' | 'phpmyadmin';
  name: string;
}

export const ipc = {
  ping: () => invoke<string>('ping'),
  listComponents: () => invoke<ComponentInfo[]>('list_components'),
  portAvailable: (port: number) => invoke<boolean>('port_available', { port }),
};
