import { useEffect, useMemo, useRef, useState } from "react";
import type {
  Business,
  EmailCsvResult,
  LicenseEntitlement,
  SearchControlAction,
  SearchJob,
  SearchRequest,
  UsageAuthorization,
} from "../domain/models";
import { getErrorMessage } from "../shared/errors";
import { licenseService } from "../services/licenseService";
import { searchService } from "../services/searchService";

function upsertById<T extends { id: string | number }>(items: T[], incoming: T): T[] {
  const index = items.findIndex((item) => item.id === incoming.id);
  if (index === -1) return [incoming, ...items];
  return items.map((item, itemIndex) => (itemIndex === index ? incoming : item));
}

export function useLeadCollection() {
  const [jobs, setJobs] = useState<SearchJob[]>([]);
  const [businesses, setBusinesses] = useState<Business[]>([]);
  const [selectedJobId, setSelectedJobId] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [isScrapingCsv, setIsScrapingCsv] = useState(false);
  const [entitlement, setEntitlement] = useState<LicenseEntitlement | null>(null);
  const [isLoadingLicense, setIsLoadingLicense] = useState(true);
  const [notice, setNotice] = useState("");
  const selectedJobIdRef = useRef("");
  const selectionVersionRef = useRef(0);

  const selectedJob = useMemo(
    () => jobs.find((job) => job.id === selectedJobId) ?? jobs[0],
    [jobs, selectedJobId],
  );

  function selectJobLocally(jobId: string) {
    selectedJobIdRef.current = jobId;
    setSelectedJobId(jobId);
  }

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void searchService
      .getSnapshot()
      .then((snapshot) => {
        if (disposed) return;
        const initialJobId = snapshot.jobs[0]?.id ?? "";
        selectedJobIdRef.current = initialJobId;
        setJobs(snapshot.jobs);
        setBusinesses(snapshot.businesses);
        setSelectedJobId(initialJobId);
      })
      .catch((error) => {
        if (!disposed) setNotice(getErrorMessage(error));
      });

    void searchService.onJobProgress((job) => {
      if (!disposed) setJobs((current) => upsertById(current, job));
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });

    void searchService.onBusinessUpsert((business) => {
      if (!disposed && business.jobId === selectedJobIdRef.current) {
        setBusinesses((current) => upsertById(current, business));
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlisteners.push(unlisten);
    });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    setIsLoadingLicense(true);
    void licenseService
      .getEntitlement()
      .then((nextEntitlement) => {
        if (!disposed) setEntitlement(nextEntitlement);
      })
      .catch((error) => {
        if (!disposed) setNotice(getErrorMessage(error));
      })
      .finally(() => {
        if (!disposed) setIsLoadingLicense(false);
      });

    return () => {
      disposed = true;
    };
  }, []);

  async function selectJob(jobId: string) {
    const selectionVersion = ++selectionVersionRef.current;
    selectJobLocally(jobId);
    try {
      const nextBusinesses = await searchService.listBusinesses(jobId);
      if (selectionVersion === selectionVersionRef.current) {
        setBusinesses(nextBusinesses);
      }
    } catch (error) {
      if (selectionVersion === selectionVersionRef.current) {
        setNotice(getErrorMessage(error));
      }
    }
  }

  async function updateBusinessEmails(businessId: number, emails: string[]) {
    try {
      const business = await searchService.updateBusinessEmails(businessId, emails);
      setBusinesses((current) => upsertById(current, business));
    } catch (error) {
      const message = getErrorMessage(error);
      setNotice(message);
      throw new Error(message);
    }
  }

  async function deleteBusiness(business: Business): Promise<boolean> {
    try {
      await searchService.deleteBusiness(business.id);
      setBusinesses((current) => current.filter((item) => item.id !== business.id));
      const selectedJobId = selectedJobIdRef.current;
      try {
        const snapshot = await searchService.getSnapshot(selectedJobId || null);
        setJobs(snapshot.jobs);
        if (selectedJobId === selectedJobIdRef.current) {
          setBusinesses(snapshot.businesses);
        }
      } catch {
        // The local list is already updated; the next snapshot will refresh counts.
      }
      setNotice(`已删除商家“${business.name || "未命名商家"}”。`);
      return true;
    } catch (error) {
      setNotice(getErrorMessage(error));
      return false;
    }
  }

  async function createJob(request: SearchRequest) {
    setIsCreating(true);
    setNotice("");
    let authorization: UsageAuthorization | null = null;
    let started = false;
    try {
      authorization = await licenseService.authorize(request.maxResults);
      const job = await searchService.startSearch(request);
      started = true;

      // The crawler starts in the backend before startSearch resolves. Keep any
      // newer progress event that may already have arrived instead of replacing
      // it with the initial "queued" job returned by the command.
      selectionVersionRef.current += 1;
      selectJobLocally(job.id);
      setJobs((current) =>
        current.some((item) => item.id === job.id) ? current : [job, ...current],
      );
      setBusinesses([]);

      try {
        await licenseService.commit(authorization.token);
        setEntitlement(await licenseService.getEntitlement());
      } catch {
        setNotice("任务已创建，但授权状态同步稍有延迟，请稍后刷新。 ");
      }

      // Reconcile events that may have completed while the authorization
      // service request was in flight (especially very small search jobs).
      try {
        const snapshot = await searchService.getSnapshot(job.id);
        setJobs(snapshot.jobs);
        if (selectedJobIdRef.current === job.id) {
          setBusinesses(snapshot.businesses);
        }
      } catch {
        // Live crawler events continue to update the interface.
      }
    } catch (error) {
      if (authorization && !started) {
        await licenseService.release(authorization.token).catch(() => undefined);
      }
      setNotice(getErrorMessage(error));
    } finally {
      setIsCreating(false);
    }
  }

  async function redeemActivationCode(code: string) {
    const nextEntitlement = await licenseService.redeem(code);
    setEntitlement(nextEntitlement);
    setNotice("激活卡兑换成功。");
  }

  async function controlJob(action: SearchControlAction) {
    if (!selectedJob) return;
    try {
      await searchService.controlSearch(selectedJob.id, action);
    } catch (error) {
      setNotice(getErrorMessage(error));
    }
  }

  async function resumeJob() {
    if (!selectedJob) return;
    try {
      await searchService.resumeSearch(selectedJob.id);
    } catch (error) {
      setNotice(getErrorMessage(error));
    }
  }

  async function deleteJob(job: SearchJob): Promise<boolean> {
    try {
      await searchService.deleteSearch(job.id);
      selectionVersionRef.current += 1;
      const remainingJobs = jobs.filter((item) => item.id !== job.id);
      setJobs(remainingJobs);

      if (selectedJob?.id === job.id) {
        const nextJobId = remainingJobs[0]?.id ?? "";
        selectJobLocally(nextJobId);
        setBusinesses(
          nextJobId ? await searchService.listBusinesses(nextJobId) : [],
        );
      }
      setNotice(`已删除任务“${job.keyword} · ${job.location}”及其全部记录。`);
      return true;
    } catch (error) {
      setNotice(getErrorMessage(error));
      return false;
    }
  }

  async function exportJobs(jobIds: string[]): Promise<boolean> {
    try {
      const result = await searchService.exportCsv(jobIds);
      const selectedJobId = selectedJobIdRef.current;
      try {
        const snapshot = await searchService.getSnapshot(selectedJobId || null);
        setJobs(snapshot.jobs);
        if (selectedJobId === selectedJobIdRef.current) {
          setBusinesses(snapshot.businesses);
        }
      } catch {
        // Export succeeded; the next snapshot will refresh the export markers.
      }
      setNotice(
        `已合并 ${result.sourceJobs} 个任务，新增导出 ${result.exportedRecords} 条记录，去除 ${result.duplicatesRemoved} 条重复数据。文件：${result.path}`,
      );
      return true;
    } catch (error) {
      setNotice(getErrorMessage(error));
      return false;
    }
  }

  async function scrapeEmailsCsv(csvContent: string): Promise<EmailCsvResult> {
    setIsScrapingCsv(true);
    setNotice("");
    try {
      const result = await searchService.scrapeEmailsCsv(csvContent);
      setNotice(
        `邮箱抓取完成：处理 ${result.processedRecords} 条记录，找到 ${result.recordsWithEmail} 条记录的邮箱，共 ${result.emailsFound} 个；无邮箱 ${result.noEmail} 条，失败 ${result.failed} 条。`,
      );
      return result;
    } catch (error) {
      const message = getErrorMessage(error);
      setNotice(message);
      throw new Error(message);
    } finally {
      setIsScrapingCsv(false);
    }
  }

  return {
    jobs,
    businesses,
    selectedJob,
    isCreating,
    isScrapingCsv,
    entitlement,
    isLoadingLicense,
    notice,
    dismissNotice: () => setNotice(""),
    selectJob,
    updateBusinessEmails,
    deleteBusiness,
    createJob,
    redeemActivationCode,
    controlJob,
    resumeJob,
    deleteJob,
    exportJobs,
    scrapeEmailsCsv,
  };
}
