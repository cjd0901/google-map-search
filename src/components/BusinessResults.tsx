import { useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getStatusMeta } from "../domain/jobStatus";
import type { Business } from "../domain/models";

type BusinessResultsProps = {
  businesses: Business[];
  onUpdateEmails: (businessId: number, emails: string[]) => Promise<void>;
  onDeleteBusiness: (business: Business) => void;
};

function matchesSearch(business: Business, normalizedQuery: string): boolean {
  return [
    business.name,
    business.category,
    business.address,
    business.phone,
    business.website,
    business.facebookUrls.join(" "),
    business.emails.join(" "),
  ].some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
}

function openExternalUrl(url: string) {
  void openUrl(url).catch(() => {
    window.open(url, "_blank", "noopener,noreferrer");
  });
}

export function BusinessResults({ businesses, onUpdateEmails, onDeleteBusiness }: BusinessResultsProps) {
  const [searchText, setSearchText] = useState("");
  const visibleBusinesses = useMemo(() => {
    const query = searchText.trim().toLocaleLowerCase();
    return query
      ? businesses.filter((business) => matchesSearch(business, query))
      : businesses;
  }, [businesses, searchText]);

  return (
    <section className="results-card">
      <div className="results-toolbar">
        <div>
          <h2>采集结果</h2>
          <span>{businesses.length} 条记录</span>
        </div>
        <input
          value={searchText}
          onChange={(event) => setSearchText(event.target.value)}
          placeholder="搜索名称、地址、电话或邮箱"
        />
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>商家</th>
              <th>联系方式</th>
              <th>地址</th>
              <th>官网邮箱（可编辑）</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {visibleBusinesses.map((business) => (
              <BusinessRow
                key={business.id}
                business={business}
                onUpdateEmails={onUpdateEmails}
                onDeleteBusiness={onDeleteBusiness}
              />
            ))}
            {!visibleBusinesses.length && <EmptyResults />}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function BusinessRow({
  business,
  onUpdateEmails,
  onDeleteBusiness,
}: {
  business: Business;
  onUpdateEmails: (businessId: number, emails: string[]) => Promise<void>;
  onDeleteBusiness: (business: Business) => void;
}) {
  const [isEditingEmails, setIsEditingEmails] = useState(false);
  const [draftEmail, setDraftEmail] = useState("");
  const [emailError, setEmailError] = useState("");
  const [isSavingEmail, setIsSavingEmail] = useState(false);
  const [emailPendingRemoval, setEmailPendingRemoval] = useState<string | null>(null);
  const status = getStatusMeta(business.status);

  async function saveEmails(emails: string[]) {
    setEmailError("");
    setIsSavingEmail(true);
    try {
      await onUpdateEmails(business.id, emails);
    } catch (error) {
      setEmailError(error instanceof Error ? error.message : String(error));
      throw error;
    } finally {
      setIsSavingEmail(false);
    }
  }

  async function addEmail() {
    const email = draftEmail.trim().toLocaleLowerCase();
    if (!email) {
      setEmailError("请输入邮箱地址。");
      return;
    }
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
      setEmailError("请输入有效的邮箱地址。");
      return;
    }
    if (business.emails.some((item) => item.toLocaleLowerCase() === email)) {
      setEmailError("该邮箱已存在。");
      return;
    }

    try {
      await saveEmails([...business.emails, email]);
      setDraftEmail("");
    } catch {
      // Keep the draft so the user can retry after a failed save.
    }
  }

  function requestRemoveEmail(email: string) {
    setEmailError("");
    setEmailPendingRemoval(email);
  }

  async function confirmRemoveEmail(email: string) {
    setEmailPendingRemoval(null);
    try {
      await saveEmails(business.emails.filter((item) => item !== email));
    } catch {
      // The notice from the parent contains the save failure.
    }
  }
  return (
    <tr>
      <td>
        <strong className="business-name">{business.name || "未命名商家"}</strong>
        <span className="secondary">{business.category || "分类未知"}</span>
        {business.rating !== null && (
          <span className="rating">
            ★ {business.rating}
            {business.reviewCount !== null && business.reviewCount > 0
              ? ` · ${business.reviewCount}`
              : ""}
          </span>
        )}
        {business.exportedAt && <span className="exported-mark">已导出</span>}
      </td>
      <td>
        <span>{business.phone || "—"}</span>
        {business.website ? (
          <a
            className="link-text"
            href={business.website}
            rel="noreferrer"
            title={business.website}
            onClick={(event) => {
              event.preventDefault();
              openExternalUrl(business.website);
            }}
          >
            {business.website}
          </a>
        ) : (
          <span className="secondary">无官网</span>
        )}
        {business.facebookUrls.map((url) => (
          <a
            className="link-text"
            href={url}
            key={url}
            rel="noreferrer"
            title={url}
            onClick={(event) => {
              event.preventDefault();
              openExternalUrl(url);
            }}
          >
            Facebook
          </a>
        ))}
      </td>
      <td className="address-cell">{business.address || "—"}</td>
      <td>
        <div className="email-list">
          {business.emails.length ? (
            business.emails.map((email) => (
              <span className="email-chip" key={email}>
                <span>{email}</span>
                {isEditingEmails && (
                  emailPendingRemoval === email ? (
                    <span className="email-remove-confirm">
                      <span>确认删除？</span>
                      <button
                        type="button"
                        className="email-confirm-delete"
                        onClick={() => void confirmRemoveEmail(email)}
                        disabled={isSavingEmail}
                      >
                        确认
                      </button>
                      <button
                        type="button"
                        className="email-cancel-delete"
                        onClick={() => setEmailPendingRemoval(null)}
                        disabled={isSavingEmail}
                      >
                        取消
                      </button>
                    </span>
                  ) : (
                    <button
                      type="button"
                      className="email-remove"
                      onClick={() => requestRemoveEmail(email)}
                      disabled={isSavingEmail}
                      aria-label={`删除邮箱 ${email}`}
                      title="删除邮箱"
                    >
                      ×
                    </button>
                  )
                )}
              </span>
            ))
          ) : (
            <span className="secondary">暂无邮箱</span>
          )}
          <button
            type="button"
            className="email-manage-button"
            onClick={() => {
              setIsEditingEmails((current) => !current);
              setEmailError("");
            }}
            disabled={isSavingEmail}
          >
            {isEditingEmails ? "收起" : "编辑"}
          </button>
        </div>
        {isEditingEmails && (
          <div className="email-editor">
            <div className="email-add-row">
              <input
                type="email"
                value={draftEmail}
                onChange={(event) => {
                  setDraftEmail(event.target.value);
                  setEmailError("");
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void addEmail();
                  }
                }}
                placeholder="添加官网邮箱"
                disabled={isSavingEmail}
                aria-label="添加官网邮箱"
              />
              <button
                type="button"
                className="email-add-button"
                onClick={() => void addEmail()}
                disabled={isSavingEmail}
              >
                {isSavingEmail ? "保存中…" : "添加"}
              </button>
            </div>
            {emailError && <span className="email-edit-error">{emailError}</span>}
          </div>
        )}
      </td>
      <td>
        <span className={`status-pill small ${status.tone}`}>{status.label}</span>
        {business.error && (
          <span className="error-text" title={business.error}>{business.error}</span>
        )}
      </td>
      <td>
        <button
          type="button"
          className="business-delete"
          onClick={() => onDeleteBusiness(business)}
          aria-label={`删除商家 ${business.name || "未命名商家"}`}
        >
          删除
        </button>
      </td>
    </tr>
  );
}

function EmptyResults() {
  return (
    <tr>
      <td colSpan={6} className="empty-table">
        <span className="empty-icon">⌖</span>
        <strong>还没有商家数据</strong>
        <span>创建任务后，采集结果会实时出现在这里。</span>
      </td>
    </tr>
  );
}
