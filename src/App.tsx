import { useEffect, useMemo, useState } from "react";
import { BusinessResults } from "./components/BusinessResults";
import { BusinessDeleteDialog } from "./components/BusinessDeleteDialog";
import { DeleteDialog } from "./components/DeleteDialog";
import { EmailCsvDialog } from "./components/EmailCsvDialog";
import { ExportDialog } from "./components/ExportDialog";
import { LicenseDialog } from "./components/LicenseDialog";
import { SearchForm } from "./components/SearchForm";
import { Sidebar } from "./components/Sidebar";
import { TaskOverview } from "./components/TaskOverview";
import { useLeadCollection } from "./hooks/useLeadCollection";
import type { LicenseEntitlement } from "./domain/models";
import "./App.css";

function licenseButtonLabel(entitlement: LicenseEntitlement | null, now: number): string {
  if (!entitlement) return "激活卡";
  if (entitlement.plan === "count" && entitlement.paidUsesRemaining > 0) {
    return `剩余 ${entitlement.paidUsesRemaining} 次`;
  }
  if (entitlement.plan === "time" && entitlement.paidExpiresAt) {
    const remainingMs = new Date(entitlement.paidExpiresAt).getTime() - now;
    if (remainingMs > 0) {
      const remainingHours = Math.ceil(remainingMs / (60 * 60 * 1000));
      return remainingHours < 24
        ? `剩余 ${remainingHours} 小时`
        : `剩余 ${Math.ceil(remainingHours / 24)} 天`;
    }
  }
  return "激活卡";
}

function App() {
  const collection = useLeadCollection();
  const [isExportOpen, setIsExportOpen] = useState(false);
  const [isEmailCsvOpen, setIsEmailCsvOpen] = useState(false);
  const [isLicenseOpen, setIsLicenseOpen] = useState(false);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    if (collection.entitlement?.plan !== "time") return;
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, [collection.entitlement?.plan]);
  const [deleteJobId, setDeleteJobId] = useState("");
  const [deleteBusinessId, setDeleteBusinessId] = useState<number | null>(null);

  const exportableJobs = useMemo(
    () => collection.jobs.filter((job) => job.businessCount > 0),
    [collection.jobs],
  );
  const deleteTarget = collection.jobs.find((job) => job.id === deleteJobId);
  const deleteBusinessTarget = collection.businesses.find(
    (business) => business.id === deleteBusinessId,
  );
  return (
    <div className="app-shell">
      <Sidebar
        jobs={collection.jobs}
        selectedJobId={collection.selectedJob?.id}
        onSelectJob={(jobId) => void collection.selectJob(jobId)}
        onDeleteJob={setDeleteJobId}
      />

      <main className="workspace">
        <header className="topbar">
          <div>
            <h1>商家信息采集</h1>
            <p>从地图结果发现商家，再从官网寻找公开联系邮箱</p>
          </div>
          <div className="topbar-actions">
            <button
              className="license-button"
              onClick={() => setIsLicenseOpen(true)}
            >
              {licenseButtonLabel(collection.entitlement, now)}
            </button>
            <button
              className="email-button"
              onClick={() => setIsEmailCsvOpen(true)}
              disabled={collection.isScrapingCsv}
            >
              {collection.isScrapingCsv ? "获取中…" : "获取邮箱"}
            </button>
            <button
              className="ghost-button"
              onClick={() => setIsExportOpen(true)}
              disabled={!exportableJobs.length}
            >
              导出 CSV
            </button>
          </div>
        </header>

        {collection.notice && (
          <div className="notice">
            <span>{collection.notice}</span>
            <button onClick={collection.dismissNotice}>×</button>
          </div>
        )}

        <SearchForm
          isCreating={collection.isCreating}
          maxResultsLimit={collection.entitlement?.maxResults ?? 10}
          onSubmit={(request) => void collection.createJob(request)}
        />

        {collection.selectedJob ? (
          <>
            <TaskOverview
              job={collection.selectedJob}
              onControl={(action) => void collection.controlJob(action)}
              onResume={() => void collection.resumeJob()}
            />
            <BusinessResults
              businesses={collection.businesses}
              onUpdateEmails={collection.updateBusinessEmails}
              onDeleteBusiness={(business) => setDeleteBusinessId(business.id)}
            />
          </>
        ) : (
          <section className="welcome-card">
            <div className="welcome-icon">⌖</div>
            <h2>创建第一个采集任务</h2>
            <p>填写关键词与目标地区，应用会打开浏览器搜索商家，并继续访问官网发现公开邮箱。</p>
          </section>
        )}
      </main>

      {isExportOpen && (
        <ExportDialog
          jobs={exportableJobs}
          onClose={() => setIsExportOpen(false)}
          onExport={collection.exportJobs}
        />
      )}

      {isEmailCsvOpen && (
        <EmailCsvDialog
          isRunning={collection.isScrapingCsv}
          onClose={() => setIsEmailCsvOpen(false)}
          onRun={collection.scrapeEmailsCsv}
        />
      )}

      {isLicenseOpen && (
        <LicenseDialog
          entitlement={collection.entitlement}
          isLoading={collection.isLoadingLicense}
          onClose={() => setIsLicenseOpen(false)}
          onRedeem={collection.redeemActivationCode}
        />
      )}

      {deleteTarget && (
        <DeleteDialog
          job={deleteTarget}
          onClose={() => setDeleteJobId("")}
          onConfirm={collection.deleteJob}
        />
      )}

      {deleteBusinessTarget && (
        <BusinessDeleteDialog
          business={deleteBusinessTarget}
          onClose={() => setDeleteBusinessId(null)}
          onConfirm={collection.deleteBusiness}
        />
      )}
    </div>
  );
}

export default App;
