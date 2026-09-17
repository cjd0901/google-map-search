import { getJobStatusMeta } from "../domain/jobStatus";
import type { SearchJob } from "../domain/models";

type SidebarProps = {
  jobs: SearchJob[];
  selectedJobId?: string;
  onSelectJob: (jobId: string) => void;
  onDeleteJob: (jobId: string) => void;
};

export function Sidebar({
  jobs,
  selectedJobId,
  onSelectJob,
  onDeleteJob,
}: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <img className="brand-mark" src="/app-icon.png" alt="" />
        <div>
          <strong>迎风数据</strong>
          <small>yingfeng-data</small>
        </div>
      </div>

      <button
        className="new-task"
        onClick={() => document.getElementById("keyword")?.focus()}
      >
        <span>＋</span> 新建任务
      </button>

      <div className="sidebar-heading">任务记录</div>
      <nav className="job-list">
        {jobs.map((job) => {
          const status = getJobStatusMeta(job);
          return (
            <div
              key={job.id}
              className={`job-item ${job.id === selectedJobId ? "active" : ""}`}
            >
              <button className="job-select" onClick={() => onSelectJob(job.id)}>
                <span className="job-title-row">
                  <span className={`status-dot ${status.tone}`} />
                  <span className="job-title">{job.keyword}</span>
                </span>
                <span className="job-location">{job.location}</span>
              </button>
              <button
                className="job-delete"
                title="删除任务"
                aria-label={`删除任务 ${job.keyword}`}
                onClick={() => onDeleteJob(job.id)}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M4 7h16M9 7V4h6v3m3 0-1 13H7L6 7m4 4v5m4-5v5" />
                </svg>
              </button>
            </div>
          );
        })}
        {!jobs.length && <div className="empty-jobs">暂无任务</div>}
      </nav>

    </aside>
  );
}
