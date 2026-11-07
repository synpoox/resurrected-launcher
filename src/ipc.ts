import { invoke } from "@tauri-apps/api/core";

/** Must equal `contract::CONTRACT_VERSION` in Rust. */
export const CONTRACT_VERSION = 1;

/** Mirrors `contract::Versioned<T>`. */
export interface Versioned<T> {
  contract_version: number;
  data: T;
}

/** Mirrors `contract::PingResponse`. */
export interface PingResponse {
  app_version: string;
  display_name: string;
}

export async function ping(): Promise<PingResponse> {
  const reply = await invoke<Versioned<PingResponse>>("ping");
  if (reply.contract_version !== CONTRACT_VERSION) {
    throw new Error(`Unsupported contract version ${reply.contract_version}`);
  }
  return reply.data;
}
