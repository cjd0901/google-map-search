import { useMemo, useState, type FormEvent } from "react";
import { getJobStatusMeta } from "../domain/jobStatus";
import type { SearchJob } from "../domain/models";

type ExportDialogProps = {
  jobs: SearchJob[];
  onClose: () => void;
  onExport: (jobIds: string[]) => Promise<boolean>;
};

export function ExportDialog({ jobs, onClose, onExport }: ExportDialogProps) {
  const [selectedJobIds, setSelectedJobIds] = useState(
    () => jobs.filter((job) => job.exported < job.businessCount).map((job) => job.id),
  );
  const [isExporting, setIsExporting] = useState(false);
  const selectedRecords = useMemo(
    () => jobs
      .filter((job) => selectedJobIds.includes(job.id))
      .reduce((total, job) => total + Math.max(job.businessCount - job.exported, 0), 0),
    [jobs, selectedJobIds],
  );
  const allSelected = selectedJobIds.length === jobs.length;

  function toggleJob(jobId: string) {
    setSelectedJobIds((current) =>
      current.includes(jobId)
        ? current.filter((id) => id !== jobId)
        : [...current, jobId],
    );
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedJobIds.length) return;
    setIsExporting(true);
    const exported = await onExport(selectedJobIds);
    setIsExporting(false);
    if (exported) onClose();
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !isExporting && onClose()}>
      <form
        className="export-dialog"
        onSubmit={submit}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="dialog-header">
          <div>
            <h2>合并导出任务记录</h2>
            <p>默认仅勾选包含未导出记录的任务；导出时会自动合并商家并去除重复项。</p>
          </div>
          <button type="button" onClick={onClose} disabled={isExporting}>×</button>
        </div>

        <div className="dialog-toolbar">
          <span>已选择 {selectedJobIds.length}/{jobs.length} 个任务</span>
          <button
            type="button"
            onClick={() => setSelectedJobIds(allSelected ? [] : jobs.map((job) => job.id))}
          >
            {allSelected ? "取消全选" : "全选"}
          </button>
        </div>

        <div className="export-job-list">
          {jobs.map((job) => {
            const checked = selectedJobIds.includes(job.id);
            const status = getJobStatusMeta(job);
            return (
              <label key={job.id} className={`export-job ${checked ? "checked" : ""}`}>
                <input
                  type="checkbox"
                  checked={checked}
                  onChange={() => toggleJob(job.id)}
                />
                <span className="export-check">✓</span>
                <span className="export-job-main">
                  <strong>{job.keyword}</strong>
                  <small>{job.location} · {new Date(job.createdAt).toLocaleString()}</small>
                </span>
                <span className={`status-pill ${status.tone}`}>{status.label}</span>
                <span className="export-count">
                  {Math.max(job.businessCount - job.exported, 0)} 条待导出
                  {job.exported > 0 && <small>已导出 {job.exported} 条</small>}
                </span>
              </label>
            );
          })}
        </div>

        <div className="dedupe-note">
          <strong>去重规则</strong>
          <span>优先识别同一个 Maps 商家，其次匹配名称、地址和电话；重复记录中的邮箱会合并保留。</span>
        </div>

        <div className="dialog-footer">
          <div>
            <span>合并前预计</span>
            <strong>{selectedRecords} 条</strong>
            <small>导出完成后会显示实际去重数量</small>
          </div>
          <button type="button" className="dialog-cancel" onClick={onClose} disabled={isExporting}>
            取消
          </button>
          <button
            type="submit"
            className="primary-button"
            disabled={!selectedJobIds.length || !selectedRecords || isExporting}
          >
            {isExporting ? "正在合并…" : `合并导出 ${selectedJobIds.length} 个任务`}
          </button>
        </div>
      </form>
    </div>
  );
}
