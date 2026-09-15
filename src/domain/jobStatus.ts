import type { JobStatus, SearchJob } from "./models";

export type StatusTone = "neutral" | "blue" | "amber" | "green" | "red" | "purple";

export type StatusMeta = {
  label: string;
  tone: StatusTone;
};

const fallbackStatus: StatusMeta = { label: "未知状态", tone: "neutral" };

const statusMeta: Record<string, StatusMeta> = {
  queued: { label: "等待中", tone: "neutral" },
  running: { label: "采集中", tone: "blue" },
  blocked: { label: "等待验证", tone: "amber" },
  paused: { label: "已暂停", tone: "amber" },
  cancelled: { label: "已取消", tone: "neutral" },
  completed: { label: "已完成", tone: "green" },
  failed: { label: "失败", tone: "red" },
  interrupted: { label: "已中断", tone: "amber" },
  pending: { label: "待处理官网", tone: "neutral" },
  crawling: { label: "官网处理中", tone: "blue" },
  no_website: { label: "无官网", tone: "neutral" },
  no_email: { label: "未发现邮箱", tone: "amber" },
};

export function getStatusMeta(status: string): StatusMeta {
  return statusMeta[status] ?? { ...fallbackStatus, label: status || fallbackStatus.label };
}

export function getJobStatusMeta(
  job: Pick<SearchJob, "status" | "exported">,
): StatusMeta {
  const status = getStatusMeta(job.status);
  return job.exported > 0 ? { ...status, tone: "purple" } : status;
}

export function isJobRunning(status: JobStatus): boolean {
  return status === "running" || status === "blocked";
}

export function isJobResumable(status: JobStatus): boolean {
  return status === "paused" || status === "interrupted" || status === "failed";
}
