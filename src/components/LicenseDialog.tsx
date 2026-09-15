import { useState } from "react";
import type { LicenseEntitlement } from "../domain/models";

type LicenseDialogProps = {
  entitlement: LicenseEntitlement | null;
  isLoading: boolean;
  onClose: () => void;
  onRedeem: (code: string) => Promise<void>;
};

function planLabel(entitlement: LicenseEntitlement | null): string {
  if (!entitlement) return "未连接授权服务";
  if (entitlement.plan === "time") return "时间授权";
  if (entitlement.plan === "count") return "次数授权";
  if (entitlement.plan === "free") return "免费试用";
  return "未激活";
}

function formatExpiration(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function LicenseDialog({
  entitlement,
  isLoading,
  onClose,
  onRedeem,
}: LicenseDialogProps) {
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [isRedeeming, setIsRedeeming] = useState(false);

  async function redeem() {
    const value = code.trim();
    if (!value) {
      setError("请输入激活卡密。");
      return;
    }
    setError("");
    setIsRedeeming(true);
    try {
      await onRedeem(value);
      setCode("");
    } catch (redeemError) {
      setError(redeemError instanceof Error ? redeemError.message : String(redeemError));
    } finally {
      setIsRedeeming(false);
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !isRedeeming && onClose()}>
      <section
        className="license-dialog"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="dialog-header">
          <div>
            <h2>激活卡</h2>
            <p>兑换次数卡或时间卡，解锁更多采集额度。</p>
          </div>
          <button type="button" onClick={onClose} disabled={isRedeeming} aria-label="关闭">
            ×
          </button>
        </div>

        <div className="license-body">
          <div className="license-status-card">
            <div className="license-status-line">
              <span>当前授权</span>
              <strong>{isLoading ? "读取中…" : planLabel(entitlement)}</strong>
            </div>
            <div className="license-status-line">
              <span>剩余次数</span>
              <strong>
                {entitlement?.plan === "time"
                  ? "时间内不限次数"
                  : entitlement?.paidUsesRemaining ?? 0}
              </strong>
            </div>
            <div className="license-status-line">
              <span>单次最大数量</span>
              <strong>{entitlement?.maxResults ?? 10}</strong>
            </div>
            {entitlement?.paidExpiresAt && (
              <div className="license-status-line">
                <span>有效期至</span>
                <strong>{formatExpiration(entitlement.paidExpiresAt)}</strong>
              </div>
            )}
          </div>

          <label className="license-code-field">
            <span>输入激活卡密</span>
            <input
              value={code}
              onChange={(event) => {
                setCode(event.target.value);
                setError("");
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void redeem();
                }
              }}
              placeholder="例如：GMS-XXXX-XXXX"
              disabled={isRedeeming}
              autoFocus
            />
          </label>
          {error && <div className="license-error">{error}</div>}
          <div className="license-note">
            免费试用仅限一次，单次最多 10 条；兑换次数卡后按次数使用，兑换时间卡后在有效期内使用。
          </div>
        </div>

        <div className="dialog-footer">
          <div>
            <span>设备授权</span>
            <small>{entitlement?.deviceId ?? "等待服务端分配设备标识"}</small>
          </div>
          <button type="button" className="dialog-cancel" onClick={onClose} disabled={isRedeeming}>
            取消
          </button>
          <button
            type="button"
            className="primary-button"
            onClick={() => void redeem()}
            disabled={isRedeeming || isLoading}
          >
            {isRedeeming ? "兑换中…" : "立即兑换"}
          </button>
        </div>
      </section>
    </div>
  );
}
