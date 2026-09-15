export type JobStatus =
  | "queued"
  | "running"
  | "blocked"
  | "paused"
  | "cancelled"
  | "completed"
  | "failed"
  | "interrupted";

export type SearchRequest = {
  keyword: string;
  location: string;
  maxResults: number;
  language: string;
  headless: boolean;
};

export type SearchJob = SearchRequest & {
  id: string;
  status: JobStatus;
  discovered: number;
  enriched: number;
  emailsFound: number;
  failed: number;
  businessCount: number;
  exported: number;
  message: string;
  createdAt: string;
  updatedAt: string;
};

export type Business = {
  id: number;
  jobId: string;
  name: string;
  category: string;
  address: string;
  phone: string;
  website: string;
  mapsUrl: string;
  rating: number | null;
  reviewCount: number | null;
  businessSummary: string;
  emails: string[];
  emailSourceUrls: string[];
  facebookUrls: string[];
  status: string;
  error: string;
  exportedAt: string | null;
};

export type Snapshot = {
  jobs: SearchJob[];
  businesses: Business[];
};

export type ExportResult = {
  path: string;
  sourceJobs: number;
  inputRecords: number;
  exportedRecords: number;
  duplicatesRemoved: number;
};

export type EmailCsvResult = {
  csv: string;
  processedRecords: number;
  recordsWithEmail: number;
  emailsFound: number;
  noEmail: number;
  failed: number;
};

export type SearchControlAction = "pause" | "cancel";

export type LicenseEntitlement = {
  deviceId: string;
  freeUsed: boolean;
  paidUsesRemaining: number;
  paidExpiresAt: string | null;
  plan: "free" | "count" | "time" | "none";
  canSearch: boolean;
  maxResults: number;
};

export type UsageAuthorization = {
  token: string;
  plan: "free" | "count" | "time";
  maxResults: number;
  paidUsesRemaining: number;
  paidExpiresAt: string | null;
  expiresAt: string;
};
