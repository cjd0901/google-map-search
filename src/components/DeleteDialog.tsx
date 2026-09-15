import { useState } from "react";
import { isJobRunning } from "../domain/jobStatus";
import type { SearchJob } from "../domain/models";

type DeleteDialogProps = {
  job: SearchJob;
  onClose: () => void;
  onConfirm: (job: SearchJob) => Promise<boolean>;
};

export function DeleteDialog({ job, onClose, onConfirm }: DeleteDialogProps) {
  const [isDeleting, setIsDeleting] = useState(false);

  async function confirm() {
    setIsDeleting(true);
    const deleted = await onConfirm(job);
    setIsDeleting(false);
    if (deleted) onClose();
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !isDeleting && onClose()}>
      <div className="delete-dialog" onMouseDown={(event) => event.stopPropagation()}>
        <div className="delete-symbol">×</div>
        <h2>删除这个任务？</h2>
        <p>
          将删除任务“<strong>{job.keyword} · {job.location}</strong>”以及该任务采集的全部商家记录。
        </p>
        {isJobRunning(job.status) && (
          <div className="delete-warning">该任务仍在运行，删除时会同时停止浏览器采集。</div>
        )}
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
