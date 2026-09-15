import { useState, type FormEvent } from "react";
import type { SearchRequest } from "../domain/models";

const DEFAULT_MAX_RESULTS = 20;
const FIXED_SEARCH_OPTIONS = {
  language: "zh-CN",
  headless: true,
} as const;

type SearchFormProps = {
  isCreating: boolean;
  maxResultsLimit: number;
  onSubmit: (request: SearchRequest) => void;
};

export function SearchForm({ isCreating, maxResultsLimit, onSubmit }: SearchFormProps) {
  const [keyword, setKeyword] = useState("");
  const [location, setLocation] = useState("");
  const [maxResults, setMaxResults] = useState(
    Math.min(DEFAULT_MAX_RESULTS, maxResultsLimit),
  );

  function changeMaxResults(value: number) {
    setMaxResults(Math.min(Math.max(value || 1, 1), maxResultsLimit));
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onSubmit({ keyword, location, maxResults, ...FIXED_SEARCH_OPTIONS });
  }

  return (
    <section className="search-card">
      <form onSubmit={submit}>
        <div className="field keyword-field">
          <label htmlFor="keyword">商家关键词</label>
          <input
            id="keyword"
            value={keyword}
            onChange={(event) => setKeyword(event.target.value)}
            placeholder="例如：咖啡馆、牙科诊所、家具批发"
            required
          />
        </div>
        <div className="field">
          <label htmlFor="location">目标地区</label>
          <input
            id="location"
            value={location}
            onChange={(event) => setLocation(event.target.value)}
            placeholder="例如：上海市静安区"
            required
          />
        </div>
        <div className="field compact">
          <label htmlFor="max-results">最大数量</label>
          <input
            id="max-results"
            type="number"
            min={1}
            max={maxResultsLimit}
            value={maxResults}
            onChange={(event) => changeMaxResults(Number(event.target.value))}
          />
        </div>
        <button className="primary-button" type="submit" disabled={isCreating}>
          {isCreating ? "创建中…" : "开始采集"}
        </button>
      </form>
    </section>
  );
}
