import { invoke } from "@tauri-apps/api/core";
import type { LicenseEntitlement, UsageAuthorization } from "../domain/models";

const API_BASE_URL = (import.meta.env.VITE_SERVER_URL || "https://wa.sililand.com:39128/gs").replace(
  /\/+$/,
  "",
);
const DEVICE_ID_KEY = "yingfeng-data.device-id";
const LEGACY_DEVICE_ID_KEY = "google-map-search.device-id";

export class LicenseServiceError extends Error {
  readonly code: string;

  constructor(message: string, code = "LICENSE_ERROR") {
    super(message);
    this.name = "LicenseServiceError";
    this.code = code;
  }
}

function createDeviceId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `device-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function getLegacyDeviceId(): string | null {
  try {
    const stored = window.localStorage.getItem(DEVICE_ID_KEY)?.trim();
    if (stored) return stored;
    const legacyStored = window.localStorage.getItem(LEGACY_DEVICE_ID_KEY)?.trim();
    if (legacyStored) {
      window.localStorage.setItem(DEVICE_ID_KEY, legacyStored);
      return legacyStored;
    }
  } catch {
    // Native persistence below remains available when WebView storage is not.
  }
  return null;
}

function saveLegacyDeviceId(deviceId: string) {
  try {
    window.localStorage.setItem(DEVICE_ID_KEY, deviceId);
  } catch {
    // The native database is the source of truth.
  }
}

let deviceIdPromise: Promise<string> | null = null;

function getDeviceId(): Promise<string> {
  if (deviceIdPromise) return deviceIdPromise;

  const legacyDeviceId = getLegacyDeviceId();
  deviceIdPromise = invoke<string>("get_or_create_device_id", { legacyDeviceId })
    .then((deviceId) => {
      saveLegacyDeviceId(deviceId);
      return deviceId;
    })
    .catch(() => {
      const fallback = legacyDeviceId || createDeviceId();
      saveLegacyDeviceId(fallback);
      return fallback;
    });
  return deviceIdPromise;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(init?.headers ?? {}),
      },
    });
  } catch {
    throw new LicenseServiceError("授权服务暂时不可用，请确认 google-map-server 已启动。", "NETWORK_ERROR");
  }

  const payload = (await response.json().catch(() => ({}))) as {
    message?: string;
    code?: string;
  };
  if (!response.ok) {
    throw new LicenseServiceError(
      payload.message || `授权服务请求失败（${response.status}）`,
      payload.code,
    );
  }
  return payload as T;
}

export const licenseService = {
  getDeviceId,

  async getEntitlement(): Promise<LicenseEntitlement> {
    const deviceId = encodeURIComponent(await getDeviceId());
    return request(`/api/v1/entitlement?deviceId=${deviceId}`);
  },

  async authorize(requestedCount: number): Promise<UsageAuthorization> {
    return request("/api/v1/usage/authorize", {
      method: "POST",
      body: JSON.stringify({ deviceId: await getDeviceId(), requestedCount }),
    });
  },

  async commit(token: string): Promise<{ ok: boolean }> {
    return request("/api/v1/usage/commit", {
      method: "POST",
      body: JSON.stringify({ deviceId: await getDeviceId(), token }),
    });
  },

  async release(token: string): Promise<{ ok: boolean }> {
    return request("/api/v1/usage/release", {
      method: "POST",
      body: JSON.stringify({ deviceId: await getDeviceId(), token }),
    });
  },

  async redeem(code: string): Promise<LicenseEntitlement> {
    return request("/api/v1/activation/redeem", {
      method: "POST",
      body: JSON.stringify({ deviceId: await getDeviceId(), code }),
    });
  },
};
