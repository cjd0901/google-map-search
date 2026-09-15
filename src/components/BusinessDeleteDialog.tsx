import { useState } from "react";
import type { Business } from "../domain/models";

type BusinessDeleteDialogProps = {
  business: Business;
  onClose: () => void;
  onConfirm: (business: Business) => Promise<boolean>;
};

export function BusinessDeleteDialog({
  business,
  onClose,
  onConfirm,
}: BusinessDeleteDialogProps) {
  const [isDeleting, setIsDeleting] = useState(false);

  async function confirm() {
    setIsDeleting(true);
    const deleted = await onConfirm(business);
    setIsDeleting(false);
    if (deleted) onClose();
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !isDeleting && onClose()}>
      <div className="delete-dialog" onMouseDown={(event) => event.stopPropagation()}>
        <div className="delete-symbol">×</div>
        <h2>删除这个商家？</h2>
        <p>
          将删除商家“<strong>{business.name || "未命名商家"}</strong>”及其已采集的信息。
        </p>
        <div className="delete-actions">
          <button type="button" onClick={onClose} disabled={isDeleting}>取消</button>
          <button
            type="button"
            className="confirm-delete"
            onClick={() => void confirm()}
            disabled={isDeleting}
          >
            {isDeleting ? "正在删除…" : "确认删除"}
          </button>
        </div>
      </div>
    </div>
  );
}
