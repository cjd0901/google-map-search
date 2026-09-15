import type { LicenseEntitlement, UsageAuthorization } from "../domain/models";

const API_BASE_URL = (import.meta.env.VITE_SERVER_URL || "https://wa.sililand.com:39128/gs").replace(
  /\/+$/,
  "",
);
const DEVICE_ID_KEY = "google-map-search.device-id";

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

function getDeviceId(): string {
  try {
    const stored = window.localStorage.getItem(DEVICE_ID_KEY)?.trim();
    if (stored) return stored;
    const deviceId = createDeviceId();
    window.localStorage.setItem(DEVICE_ID_KEY, deviceId);
    return deviceId;
  } catch {
    return createDeviceId();
  }
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

  getEntitlement(): Promise<LicenseEntitlement> {
    const deviceId = encodeURIComponent(getDeviceId());
    return request(`/api/v1/entitlement?deviceId=${deviceId}`);
  },

  authorize(requestedCount: number): Promise<UsageAuthorization> {
    return request("/api/v1/usage/authorize", {
      method: "POST",
      body: JSON.stringify({ deviceId: getDeviceId(), requestedCount }),
    });
  },

  commit(token: string): Promise<{ ok: boolean }> {
    return request("/api/v1/usage/commit", {
      method: "POST",
      body: JSON.stringify({ deviceId: getDeviceId(), token }),
    });
  },

  release(token: string): Promise<{ ok: boolean }> {
    return request("/api/v1/usage/release", {
      method: "POST",
      body: JSON.stringify({ deviceId: getDeviceId(), token }),
    });
  },

  redeem(code: string): Promise<LicenseEntitlement> {
    return request("/api/v1/activation/redeem", {
      method: "POST",
      body: JSON.stringify({ deviceId: getDeviceId(), code }),
    });
  },
};
