import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Business,
  EmailCsvResult,
  ExportResult,
  SearchControlAction,
  SearchJob,
  SearchRequest,
  Snapshot,
} from "../domain/models";

export const searchService = {
  getSnapshot(jobId: string | null = null): Promise<Snapshot> {
    return invoke("get_snapshot", { jobId });
  },

  listBusinesses(jobId: string): Promise<Business[]> {
    return invoke("list_businesses", { jobId });
  },

  updateBusinessEmails(businessId: number, emails: string[]): Promise<Business> {
    return invoke("update_business_emails", { businessId, emails });
  },

  deleteBusiness(businessId: number): Promise<void> {
    return invoke("delete_business", { businessId });
  },

  startSearch(request: SearchRequest): Promise<SearchJob> {
    return invoke("start_search", { request });
  },

  controlSearch(jobId: string, action: SearchControlAction): Promise<void> {
    return invoke("control_search", { jobId, action });
  },

  resumeSearch(jobId: string): Promise<SearchJob> {
    return invoke("resume_search", { jobId });
  },

  deleteSearch(jobId: string): Promise<void> {
    return invoke("delete_search", { jobId });
  },

  exportCsv(jobIds: string[]): Promise<ExportResult> {
    return invoke("export_csv", { jobIds });
  },

  scrapeEmailsCsv(csvContent: string): Promise<EmailCsvResult> {
    return invoke("scrape_emails_csv", { csvContent });
  },

  getDiagnosticLogPath(): Promise<string> {
    return invoke("get_diagnostic_log_path");
  },

  writeDiagnosticLog(source: string, message: string): Promise<void> {
    return invoke("write_diagnostic_log", { source, message });
  },

  onJobProgress(listener: (job: SearchJob) => void): Promise<UnlistenFn> {
    return listen<SearchJob>("job-progress", ({ payload }) => listener(payload));
  },

  onBusinessUpsert(listener: (business: Business) => void): Promise<UnlistenFn> {
    return listen<Business>("business-upsert", ({ payload }) => listener(payload));
  },
};
