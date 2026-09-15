import {
  getJobStatusMeta,
  isJobResumable,
  isJobRunning,
} from "../domain/jobStatus";
import type { SearchControlAction, SearchJob } from "../domain/models";

type TaskOverviewProps = {
  job: SearchJob;
  onControl: (action: SearchControlAction) => void;
  onResume: () => void;
};

function calculateProgress(job: SearchJob): number {
  return Math.min(
    100,
    Math.round((job.enriched / Math.max(job.discovered, 1)) * 100),
  );
}

export function TaskOverview({ job, onControl, onResume }: TaskOverviewProps) {
  const status = getJobStatusMeta(job);
  const running = isJobRunning(job.status);

  return (
    <section className="task-overview">
      <div className="task-summary">
        <div className="task-heading">
          <div>
            <span className={`status-pill ${status.tone}`}>{status.label}</span>
            <strong>{job.keyword}</strong>
            <span>{job.location}</span>
          </div>
          <div className="task-actions">
            {running && <button onClick={() => onControl("pause")}>暂停</button>}
            {isJobResumable(job.status) && <button onClick={onResume}>继续</button>}
            {running && (
              <button className="danger" onClick={() => onControl("cancel")}>
                取消
              </button>
            )}
          </div>
        </div>
        <p>{job.message}</p>
        <div className="progress-track">
          <span style={{ width: `${calculateProgress(job)}%` }} />
        </div>
      </div>
      <div className="stat"><span>发现商家</span><strong>{job.discovered}</strong></div>
      <div className="stat"><span>官网已处理</span><strong>{job.enriched}</strong></div>
      <div className="stat"><span>发现邮箱</span><strong>{job.emailsFound}</strong></div>
      <div className="stat"><span>失败</span><strong>{job.failed}</strong></div>
    </section>
  );
}
