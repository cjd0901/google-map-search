import { useRef, useState, type ChangeEvent } from "react";
import type { EmailCsvResult } from "../domain/models";

type EmailCsvDialogProps = {
  isRunning: boolean;
  onClose: () => void;
  onRun: (csvContent: string) => Promise<EmailCsvResult>;
};

function outputFileName(inputName: string): string {
  const stem = inputName.replace(/\.csv$/i, "");
  return `${stem}-with-emails.csv`;
}

function downloadCsv(csv: string, fileName: string) {
  const blob = new Blob([csv], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = outputFileName(fileName);
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

export function EmailCsvDialog({ isRunning, onClose, onRun }: EmailCsvDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<{ name: string; content: string } | null>(null);
  const [error, setError] = useState("");

  async function handleFileChange(event: ChangeEvent<HTMLInputElement>) {
    const selected = event.target.files?.[0];
    if (!selected) return;
    setError("");
    if (!selected.name.toLowerCase().endsWith(".csv")) {
      setFile(null);
      setError("请选择导出的 CSV 文件。");
      return;
    }
    try {
      const content = await selected.text();
      setFile({ name: selected.name, content });
    } catch {
      setFile(null);
      setError("无法读取该文件，请重新选择 CSV。");
    }
  }

  async function handleRun() {
    if (!file || isRunning) return;
    setError("");
    try {
      const result = await onRun(file.content);
      downloadCsv(result.csv, file.name);
      onClose();
    } catch (runError) {
      setError(runError instanceof Error ? runError.message : String(runError));
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={() => !isRunning && onClose()}>
      <section
        className="email-csv-dialog"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div className="dialog-header">
          <div>
            <h2>从导出 CSV 抓取邮箱</h2>
          </div>
          <button type="button" onClick={onClose} disabled={isRunning} aria-label="关闭">
            ×
          </button>
        </div>

        <div className="email-csv-body">
          <input
            ref={inputRef}
            type="file"
            accept=".csv,text/csv"
            onChange={(event) => void handleFileChange(event)}
            hidden
          />
          <button
            type="button"
            className="email-file-picker"
            onClick={() => inputRef.current?.click()}
            disabled={isRunning}
          >
            <span className="email-file-icon">CSV</span>
            <span>
              <strong>{file ? file.name : "选择工具导出的 CSV 文件"}</strong>
              <small>{file ? "文件已就绪，点击确定开始抓取" : "仅支持本工具导出的 CSV"}</small>
            </span>
            <span className="email-file-arrow">选择</span>
          </button>
          <div className="email-csv-note">
            只处理“邮箱”为空且有“官网”的记录，原始文件不会被覆盖，完成后会下载一份新文件。
          </div>
          {error && <div className="email-csv-error">{error}</div>}
        </div>

        <div className="dialog-footer">
          <div>
            <span>{file ? "已选择文件" : "尚未选择文件"}</span>
            <small>{isRunning ? "正在访问公开页面，请稍候…" : ""}</small>
          </div>
          <button type="button" className="dialog-cancel" onClick={onClose} disabled={isRunning}>
            取消
          </button>
          <button
            type="button"
            className="primary-button"
            onClick={() => void handleRun()}
            disabled={!file || isRunning}
          >
            {isRunning ? "抓取中…" : "确定并开始"}
          </button>
        </div>
      </section>
    </div>
  );
}
