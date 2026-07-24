import {
  ArrowDownRight,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  BarChart3,
  CalendarDays,
  Check,
  CircleAlert,
  LayoutDashboard,
  LoaderCircle,
  Pencil,
  Plus,
  ReceiptText,
  RefreshCw,
  Trash2,
  WalletCards,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import type { GameUiComponentNode, GameUiPlatform } from "../data/gameUi";
import type { WorldFrameAction } from "./protocol";
import "./LedgerBook.css";

type LedgerBookProps = {
  node?: GameUiComponentNode;
  platform: GameUiPlatform;
  sendAction: (action: WorldFrameAction) => Promise<unknown>;
};

type LedgerKind = "income" | "expense";
type LedgerView = "overview" | "transactions" | "stats";
type PeriodMode = "day" | "month" | "year" | "all";

type LedgerEntry = {
  id: string;
  kind: LedgerKind;
  amountCents: number;
  date: string;
  category: string;
  account: string;
  note: string;
  createdAt: string;
  updatedAt: string;
};

type LedgerFormState = {
  kind: LedgerKind;
  amount: string;
  date: string;
  category: string;
  account: string;
  note: string;
};

type StoredWorldRecord = {
  id: string;
  world_id: string;
  collection: string;
  data: Record<string, unknown>;
  created_at: string;
  updated_at: string;
};

type LedgerTotals = {
  income: number;
  expense: number;
  balance: number;
};

type TrendBucket = {
  key: string;
  label: string;
  income: number;
  expense: number;
};

const DEFAULT_INCOME_CATEGORIES = ["工资", "奖金", "理财", "兼职", "报销", "其他收入"];
const DEFAULT_EXPENSE_CATEGORIES = ["餐饮", "交通", "购物", "居住", "娱乐", "医疗", "教育", "人情", "其他支出"];
const DEFAULT_ACCOUNTS = ["微信", "支付宝", "银行卡", "现金"];

export function LedgerBook({ node, platform, sendAction }: LedgerBookProps) {
  const title = readStringProp(node, "title", "记账助手");
  const collection = readCollectionProp(node, "collection", "ledger.entries");
  const currency = readStringProp(node, "currency", "¥").slice(0, 8);
  const incomeCategories = readStringListProp(node, "income_categories", DEFAULT_INCOME_CATEGORIES);
  const expenseCategories = readStringListProp(node, "expense_categories", DEFAULT_EXPENSE_CATEGORIES);
  const defaultView = readLedgerView(node?.props?.default_view);

  const [entries, setEntries] = useState<LedgerEntry[]>([]);
  const [invalidRecordCount, setInvalidRecordCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [view, setView] = useState<LedgerView>(defaultView);
  const [periodMode, setPeriodMode] = useState<PeriodMode>("month");
  const [periodValue, setPeriodValue] = useState(() => currentPeriodValue("month"));
  const [kindFilter, setKindFilter] = useState<"all" | LedgerKind>("all");
  const [query, setQuery] = useState("");
  const [form, setForm] = useState<LedgerFormState | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [formError, setFormError] = useState("");
  const [saving, setSaving] = useState(false);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const loadEntries = useCallback(async () => {
    setLoading(true);
    setLoadError("");
    try {
      const result = await sendAction({ type: "world-record-list", collection });
      const decoded = decodeLedgerRecords(result);
      setEntries(sortEntries(decoded.entries));
      setInvalidRecordCount(decoded.invalidCount);
    } catch (errorLike) {
      setLoadError(readErrorMessage(errorLike, "账本加载失败，请稍后重试。"));
    } finally {
      setLoading(false);
    }
  }, [collection, sendAction]);

  useEffect(() => {
    void loadEntries();
  }, [loadEntries]);

  const periodEntries = useMemo(
    () => entries.filter((entry) => matchesPeriod(entry.date, periodMode, periodValue)),
    [entries, periodMode, periodValue],
  );
  const visibleEntries = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    return periodEntries.filter((entry) => {
      if (kindFilter !== "all" && entry.kind !== kindFilter) {
        return false;
      }
      if (!normalizedQuery) {
        return true;
      }
      return [entry.category, entry.account, entry.note, entry.date]
        .some((value) => value.toLocaleLowerCase().includes(normalizedQuery));
    });
  }, [kindFilter, periodEntries, query]);
  const totals = useMemo(() => calculateTotals(periodEntries), [periodEntries]);
  const allTotals = useMemo(() => calculateTotals(entries), [entries]);

  const openCreateForm = (kind: LedgerKind = "expense") => {
    const categories = kind === "income" ? incomeCategories : expenseCategories;
    setEditingId(null);
    setPendingDeleteId(null);
    setFormError("");
    setForm({
      kind,
      amount: "",
      date: todayIsoDate(),
      category: categories[0] ?? "其他",
      account: DEFAULT_ACCOUNTS[0],
      note: "",
    });
  };

  const openEditForm = (entry: LedgerEntry) => {
    setEditingId(entry.id);
    setPendingDeleteId(null);
    setFormError("");
    setForm({
      kind: entry.kind,
      amount: centsToInput(entry.amountCents),
      date: entry.date,
      category: entry.category,
      account: entry.account,
      note: entry.note,
    });
  };

  const closeForm = () => {
    if (saving) {
      return;
    }
    setForm(null);
    setEditingId(null);
    setFormError("");
  };

  const updateFormKind = (kind: LedgerKind) => {
    if (!form) {
      return;
    }
    const categories = kind === "income" ? incomeCategories : expenseCategories;
    setForm({ ...form, kind, category: categories[0] ?? "其他" });
  };

  const submitForm = async () => {
    if (!form || saving) {
      return;
    }
    const amountCents = parseAmountToCents(form.amount);
    if (amountCents === null) {
      setFormError("请输入大于 0 且最多保留两位小数的金额。");
      return;
    }
    if (!isValidIsoDate(form.date)) {
      setFormError("请选择有效日期。");
      return;
    }
    if (!form.category.trim()) {
      setFormError("请选择收支分类。");
      return;
    }

    const data = {
      schema_version: 1,
      kind: form.kind,
      amount_cents: amountCents,
      date: form.date,
      category: form.category.trim().slice(0, 80),
      account: form.account.trim().slice(0, 80),
      note: form.note.trim().slice(0, 500),
    };

    setSaving(true);
    setFormError("");
    try {
      const result = editingId
        ? await sendAction({
            type: "world-record-update",
            collection,
            recordId: editingId,
            data,
          })
        : await sendAction({ type: "world-record-create", collection, data });
      const stored = decodeLedgerRecord(result);
      if (!stored) {
        throw new Error("宿主返回了无法识别的账单记录。");
      }
      setEntries((previous) => sortEntries([
        stored,
        ...previous.filter((entry) => entry.id !== stored.id),
      ]));
      setForm(null);
      setEditingId(null);
    } catch (errorLike) {
      setFormError(readErrorMessage(errorLike, "账单保存失败，请稍后重试。"));
    } finally {
      setSaving(false);
    }
  };

  const deleteEntry = async (entry: LedgerEntry) => {
    if (deletingId) {
      return;
    }
    setDeletingId(entry.id);
    setLoadError("");
    try {
      await sendAction({
        type: "world-record-delete",
        collection,
        recordId: entry.id,
      });
      setEntries((previous) => previous.filter((item) => item.id !== entry.id));
      setPendingDeleteId(null);
    } catch (errorLike) {
      setLoadError(readErrorMessage(errorLike, "账单删除失败，请稍后重试。"));
    } finally {
      setDeletingId(null);
    }
  };

  const handlePeriodMode = (nextMode: PeriodMode) => {
    setPeriodMode(nextMode);
    setPeriodValue(currentPeriodValue(nextMode));
  };

  return (
    <main className={`ledger-book ledger-book--${platform}`} data-ledger-view={view}>
      <header className="ledger-header">
        <div className="ledger-brand">
          <span className="ledger-brand-icon" aria-hidden="true"><WalletCards size={22} /></span>
          <div className="ledger-brand-copy">
            <h1>{title}</h1>
            <p>{entries.length > 0 ? `${entries.length} 笔记录 · 数据保存在当前世界` : "本地账本 · 数据保存在当前世界"}</p>
          </div>
        </div>
        <button type="button" className="ledger-primary-action" onClick={() => openCreateForm("expense")}>
          <Plus size={17} />
          <span>记一笔</span>
        </button>
      </header>

      <nav className="ledger-view-tabs" aria-label="账本页面">
        <LedgerTab active={view === "overview"} icon={<LayoutDashboard size={17} />} label="总览" onClick={() => setView("overview")} />
        <LedgerTab active={view === "transactions"} icon={<ReceiptText size={17} />} label="明细" onClick={() => setView("transactions")} />
        <LedgerTab active={view === "stats"} icon={<BarChart3 size={17} />} label="统计" onClick={() => setView("stats")} />
      </nav>

      <PeriodToolbar
        mode={periodMode}
        value={periodValue}
        onModeChange={handlePeriodMode}
        onValueChange={setPeriodValue}
        onStep={(direction) => setPeriodValue((current) => stepPeriod(current, periodMode, direction))}
      />

      {loadError ? (
        <div className="ledger-notice ledger-notice--error" role="alert">
          <CircleAlert size={17} />
          <span>{loadError}</span>
          <button type="button" onClick={() => void loadEntries()}><RefreshCw size={15} />重试</button>
        </div>
      ) : null}
      {invalidRecordCount > 0 ? (
        <div className="ledger-notice" role="status">
          <CircleAlert size={17} />
          <span>{`${invalidRecordCount} 条旧数据格式不兼容，已暂不纳入统计。`}</span>
        </div>
      ) : null}

      {loading ? <LedgerLoading /> : null}
      {!loading && view === "overview" ? (
        <OverviewView
          entries={periodEntries}
          totals={totals}
          allTotals={allTotals}
          currency={currency}
          periodLabel={formatPeriodLabel(periodMode, periodValue)}
          onCreate={openCreateForm}
          onShowTransactions={() => setView("transactions")}
        />
      ) : null}
      {!loading && view === "transactions" ? (
        <TransactionsView
          entries={visibleEntries}
          currency={currency}
          kindFilter={kindFilter}
          query={query}
          pendingDeleteId={pendingDeleteId}
          deletingId={deletingId}
          onKindFilter={setKindFilter}
          onQuery={setQuery}
          onEdit={openEditForm}
          onDeleteIntent={setPendingDeleteId}
          onDelete={deleteEntry}
          onCreate={openCreateForm}
        />
      ) : null}
      {!loading && view === "stats" ? (
        <StatisticsView
          entries={periodEntries}
          totals={totals}
          currency={currency}
          periodMode={periodMode}
          periodValue={periodValue}
        />
      ) : null}

      {form ? (
        <LedgerEntryDialog
          form={form}
          editing={Boolean(editingId)}
          error={formError}
          saving={saving}
          currency={currency}
          categories={form.kind === "income" ? incomeCategories : expenseCategories}
          onChange={setForm}
          onKindChange={updateFormKind}
          onClose={closeForm}
          onSubmit={submitForm}
        />
      ) : null}
    </main>
  );
}

function LedgerTab({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={active ? "ledger-view-tab ledger-view-tab--active" : "ledger-view-tab"}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

function PeriodToolbar({
  mode,
  value,
  onModeChange,
  onValueChange,
  onStep,
}: {
  mode: PeriodMode;
  value: string;
  onModeChange: (mode: PeriodMode) => void;
  onValueChange: (value: string) => void;
  onStep: (direction: -1 | 1) => void;
}) {
  return (
    <section className="ledger-period-toolbar" aria-label="统计周期">
      <div className="ledger-segmented" role="group" aria-label="周期粒度">
        {(["day", "month", "year", "all"] as const).map((item) => (
          <button
            key={item}
            type="button"
            className={mode === item ? "is-active" : ""}
            onClick={() => onModeChange(item)}
          >
            {{ day: "日", month: "月", year: "年", all: "全部" }[item]}
          </button>
        ))}
      </div>
      <div className="ledger-period-picker">
        <button type="button" className="ledger-icon-button" onClick={() => onStep(-1)} disabled={mode === "all"} aria-label="上一个周期" title="上一个周期">
          <ArrowLeft size={16} />
        </button>
        <label className="ledger-period-input">
          <CalendarDays size={16} />
          {mode === "day" ? <input type="date" value={value} onChange={(event) => onValueChange(event.target.value)} /> : null}
          {mode === "month" ? <input type="month" value={value} onChange={(event) => onValueChange(event.target.value)} /> : null}
          {mode === "year" ? <input type="number" min="1900" max="2200" step="1" value={value} onChange={(event) => onValueChange(normalizeYear(event.target.value))} aria-label="年份" /> : null}
          {mode === "all" ? <span>全部时间</span> : null}
        </label>
        <button type="button" className="ledger-icon-button" onClick={() => onStep(1)} disabled={mode === "all"} aria-label="下一个周期" title="下一个周期">
          <ArrowRight size={16} />
        </button>
      </div>
    </section>
  );
}

function OverviewView({
  entries,
  totals,
  allTotals,
  currency,
  periodLabel,
  onCreate,
  onShowTransactions,
}: {
  entries: LedgerEntry[];
  totals: LedgerTotals;
  allTotals: LedgerTotals;
  currency: string;
  periodLabel: string;
  onCreate: (kind: LedgerKind) => void;
  onShowTransactions: () => void;
}) {
  const latest = entries.slice(0, 6);
  const expenseBreakdown = buildCategoryBreakdown(entries, "expense").slice(0, 5);
  const maxCategory = Math.max(1, ...expenseBreakdown.map((item) => item.amount));

  return (
    <div className="ledger-view ledger-overview">
      <section className="ledger-summary" aria-label={`${periodLabel}收支概览`}>
        <SummaryCard kind="income" label="收入" value={totals.income} currency={currency} icon={<ArrowDownRight size={18} />} />
        <SummaryCard kind="expense" label="支出" value={totals.expense} currency={currency} icon={<ArrowUpRight size={18} />} />
        <SummaryCard kind="balance" label="结余" value={totals.balance} currency={currency} icon={<WalletCards size={18} />} />
      </section>

      <div className="ledger-overview-grid">
        <section className="ledger-section ledger-recent-section">
          <div className="ledger-section-heading">
            <div><span className="ledger-eyebrow">{periodLabel}</span><h2>最近明细</h2></div>
            <button type="button" className="ledger-text-button" onClick={onShowTransactions}>查看全部<ArrowRight size={15} /></button>
          </div>
          {latest.length > 0 ? (
            <div className="ledger-entry-list">
              {latest.map((entry) => <LedgerEntryRow key={entry.id} entry={entry} currency={currency} compact />)}
            </div>
          ) : (
            <LedgerEmpty
              title="这个周期还没有记录"
              detail="先录入一笔收入或支出，汇总会立即更新。"
              onCreate={() => onCreate("expense")}
            />
          )}
        </section>

        <section className="ledger-section ledger-breakdown-section">
          <div className="ledger-section-heading">
            <div><span className="ledger-eyebrow">支出去向</span><h2>分类占比</h2></div>
          </div>
          {expenseBreakdown.length > 0 ? (
            <div className="ledger-category-bars">
              {expenseBreakdown.map((item) => (
                <div className="ledger-category-bar" key={item.category}>
                  <div className="ledger-category-meta"><span>{item.category}</span><strong>{formatMoney(item.amount, currency)}</strong></div>
                  <div className="ledger-category-track"><span style={{ width: `${Math.max(4, (item.amount / maxCategory) * 100)}%` }} /></div>
                </div>
              ))}
            </div>
          ) : (
            <div className="ledger-quiet-empty">暂无支出分类数据</div>
          )}
          <div className="ledger-lifetime-balance">
            <span>累计结余</span>
            <strong className={allTotals.balance < 0 ? "is-negative" : ""}>{formatSignedMoney(allTotals.balance, currency)}</strong>
          </div>
        </section>
      </div>
    </div>
  );
}

function SummaryCard({
  kind,
  label,
  value,
  currency,
  icon,
}: {
  kind: "income" | "expense" | "balance";
  label: string;
  value: number;
  currency: string;
  icon: React.ReactNode;
}) {
  return (
    <div className={`ledger-summary-card ledger-summary-card--${kind}`}>
      <div className="ledger-summary-label"><span>{icon}</span>{label}</div>
      <strong>{kind === "balance" ? formatSignedMoney(value, currency) : formatMoney(value, currency)}</strong>
    </div>
  );
}

function TransactionsView({
  entries,
  currency,
  kindFilter,
  query,
  pendingDeleteId,
  deletingId,
  onKindFilter,
  onQuery,
  onEdit,
  onDeleteIntent,
  onDelete,
  onCreate,
}: {
  entries: LedgerEntry[];
  currency: string;
  kindFilter: "all" | LedgerKind;
  query: string;
  pendingDeleteId: string | null;
  deletingId: string | null;
  onKindFilter: (kind: "all" | LedgerKind) => void;
  onQuery: (query: string) => void;
  onEdit: (entry: LedgerEntry) => void;
  onDeleteIntent: (id: string | null) => void;
  onDelete: (entry: LedgerEntry) => void;
  onCreate: (kind: LedgerKind) => void;
}) {
  const groups = groupEntriesByDate(entries);
  return (
    <section className="ledger-view ledger-transactions">
      <div className="ledger-list-toolbar">
        <div className="ledger-segmented" role="group" aria-label="收支类型">
          {(["all", "expense", "income"] as const).map((item) => (
            <button key={item} type="button" className={kindFilter === item ? "is-active" : ""} onClick={() => onKindFilter(item)}>
              {{ all: "全部", expense: "支出", income: "收入" }[item]}
            </button>
          ))}
        </div>
        <input
          className="ledger-search"
          type="search"
          value={query}
          onChange={(event) => onQuery(event.target.value)}
          placeholder="搜索分类、账户或备注"
          aria-label="搜索账单"
        />
      </div>
      {groups.length > 0 ? (
        <div className="ledger-date-groups">
          {groups.map((group) => (
            <section className="ledger-date-group" key={group.date}>
              <div className="ledger-date-heading">
                <div><strong>{formatFriendlyDate(group.date)}</strong><span>{weekdayLabel(group.date)}</span></div>
                <span>{formatDayNet(group.entries, currency)}</span>
              </div>
              <div className="ledger-entry-list">
                {group.entries.map((entry) => (
                  <div className="ledger-entry-wrap" key={entry.id}>
                    <LedgerEntryRow
                      entry={entry}
                      currency={currency}
                      actions={(
                        <>
                          <button type="button" className="ledger-icon-button" onClick={() => onEdit(entry)} aria-label="编辑账单" title="编辑账单"><Pencil size={15} /></button>
                          <button type="button" className="ledger-icon-button ledger-icon-button--danger" onClick={() => onDeleteIntent(entry.id)} aria-label="删除账单" title="删除账单"><Trash2 size={15} /></button>
                        </>
                      )}
                    />
                    {pendingDeleteId === entry.id ? (
                      <div className="ledger-delete-confirm" role="alert">
                        <span>确定删除这笔记录？</span>
                        <button type="button" onClick={() => onDeleteIntent(null)} disabled={deletingId === entry.id}>取消</button>
                        <button type="button" className="is-danger" onClick={() => onDelete(entry)} disabled={deletingId === entry.id}>
                          {deletingId === entry.id ? <LoaderCircle size={14} className="ledger-spin" /> : <Trash2 size={14} />}删除
                        </button>
                      </div>
                    ) : null}
                  </div>
                ))}
              </div>
            </section>
          ))}
        </div>
      ) : (
        <LedgerEmpty title="没有符合条件的账单" detail="调整周期或筛选条件，也可以直接录入一笔新账。" onCreate={() => onCreate("expense")} />
      )}
    </section>
  );
}

function LedgerEntryRow({
  entry,
  currency,
  compact = false,
  actions,
}: {
  entry: LedgerEntry;
  currency: string;
  compact?: boolean;
  actions?: React.ReactNode;
}) {
  return (
    <article className={compact ? "ledger-entry ledger-entry--compact" : "ledger-entry"}>
      <span className={`ledger-entry-kind ledger-entry-kind--${entry.kind}`} aria-hidden="true">
        {entry.kind === "income" ? <ArrowDownRight size={17} /> : <ArrowUpRight size={17} />}
      </span>
      <div className="ledger-entry-main">
        <div className="ledger-entry-title"><strong>{entry.category}</strong>{entry.note ? <span>{entry.note}</span> : null}</div>
        <div className="ledger-entry-meta"><span>{entry.account || "未指定账户"}</span>{compact ? <span>{formatFriendlyDate(entry.date)}</span> : null}</div>
      </div>
      <strong className={`ledger-entry-amount ledger-entry-amount--${entry.kind}`}>
        {entry.kind === "income" ? "+" : "-"}{formatMoney(entry.amountCents, currency)}
      </strong>
      {actions ? <div className="ledger-entry-actions">{actions}</div> : null}
    </article>
  );
}

function StatisticsView({
  entries,
  totals,
  currency,
  periodMode,
  periodValue,
}: {
  entries: LedgerEntry[];
  totals: LedgerTotals;
  currency: string;
  periodMode: PeriodMode;
  periodValue: string;
}) {
  const trend = buildTrend(entries, periodMode, periodValue);
  const maxTrend = Math.max(1, ...trend.flatMap((item) => [item.income, item.expense]));
  const expenses = buildCategoryBreakdown(entries, "expense").slice(0, 8);
  const incomes = buildCategoryBreakdown(entries, "income").slice(0, 8);
  const maxExpense = Math.max(1, ...expenses.map((item) => item.amount));
  const maxIncome = Math.max(1, ...incomes.map((item) => item.amount));

  return (
    <div className="ledger-view ledger-stats">
      <section className="ledger-summary" aria-label="收支统计">
        <SummaryCard kind="income" label="收入" value={totals.income} currency={currency} icon={<ArrowDownRight size={18} />} />
        <SummaryCard kind="expense" label="支出" value={totals.expense} currency={currency} icon={<ArrowUpRight size={18} />} />
        <SummaryCard kind="balance" label="结余" value={totals.balance} currency={currency} icon={<WalletCards size={18} />} />
      </section>

      <section className="ledger-section ledger-trend-section">
        <div className="ledger-section-heading">
          <div><span className="ledger-eyebrow">{formatPeriodLabel(periodMode, periodValue)}</span><h2>收支趋势</h2></div>
          <div className="ledger-chart-legend"><span className="is-income">收入</span><span className="is-expense">支出</span></div>
        </div>
        {trend.some((item) => item.income > 0 || item.expense > 0) ? (
          <div className="ledger-trend-chart" role="img" aria-label="收支趋势柱状图">
            {trend.map((item) => (
              <div className="ledger-trend-column" key={item.key} title={`${item.label}：收入 ${formatMoney(item.income, currency)}，支出 ${formatMoney(item.expense, currency)}`}>
                <div className="ledger-trend-bars">
                  <span className="is-income" style={{ height: `${Math.max(item.income > 0 ? 4 : 0, (item.income / maxTrend) * 100)}%` }} />
                  <span className="is-expense" style={{ height: `${Math.max(item.expense > 0 ? 4 : 0, (item.expense / maxTrend) * 100)}%` }} />
                </div>
                <span className="ledger-trend-label">{item.label}</span>
              </div>
            ))}
          </div>
        ) : <div className="ledger-quiet-empty">当前周期暂无可统计数据</div>}
      </section>

      <div className="ledger-stats-grid">
        <CategoryStats title="支出分类" items={expenses} max={maxExpense} total={totals.expense} currency={currency} kind="expense" />
        <CategoryStats title="收入分类" items={incomes} max={maxIncome} total={totals.income} currency={currency} kind="income" />
      </div>
    </div>
  );
}

function CategoryStats({
  title,
  items,
  max,
  total,
  currency,
  kind,
}: {
  title: string;
  items: Array<{ category: string; amount: number }>;
  max: number;
  total: number;
  currency: string;
  kind: LedgerKind;
}) {
  return (
    <section className="ledger-section ledger-category-section">
      <div className="ledger-section-heading"><div><span className="ledger-eyebrow">分类构成</span><h2>{title}</h2></div></div>
      {items.length > 0 ? (
        <div className={`ledger-category-bars ledger-category-bars--${kind}`}>
          {items.map((item) => (
            <div className="ledger-category-bar" key={item.category}>
              <div className="ledger-category-meta">
                <span>{item.category}<small>{total > 0 ? `${Math.round((item.amount / total) * 100)}%` : "0%"}</small></span>
                <strong>{formatMoney(item.amount, currency)}</strong>
              </div>
              <div className="ledger-category-track"><span style={{ width: `${Math.max(4, (item.amount / max) * 100)}%` }} /></div>
            </div>
          ))}
        </div>
      ) : <div className="ledger-quiet-empty">暂无{title}</div>}
    </section>
  );
}

function LedgerEntryDialog({
  form,
  editing,
  error,
  saving,
  currency,
  categories,
  onChange,
  onKindChange,
  onClose,
  onSubmit,
}: {
  form: LedgerFormState;
  editing: boolean;
  error: string;
  saving: boolean;
  currency: string;
  categories: string[];
  onChange: (form: LedgerFormState) => void;
  onKindChange: (kind: LedgerKind) => void;
  onClose: () => void;
  onSubmit: () => Promise<void>;
}) {
  return (
    <div className="ledger-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <section className="ledger-dialog" role="dialog" aria-modal="true" aria-labelledby="ledger-dialog-title">
        <div className="ledger-dialog-header">
          <div><span className="ledger-eyebrow">{editing ? "修改记录" : "新增记录"}</span><h2 id="ledger-dialog-title">{editing ? "编辑账单" : "记一笔"}</h2></div>
          <button type="button" className="ledger-icon-button" onClick={onClose} disabled={saving} aria-label="关闭" title="关闭"><X size={18} /></button>
        </div>
        <div
          className="ledger-dialog-form"
          onKeyDown={(event) => {
            if (event.key === "Enter" && !(event.target instanceof HTMLButtonElement)) {
              event.preventDefault();
              void onSubmit();
            }
          }}
        >
          <div className="ledger-kind-switch" role="group" aria-label="收支类型">
            <button type="button" className={form.kind === "expense" ? "is-active is-expense" : ""} onClick={() => onKindChange("expense")}><ArrowUpRight size={17} />支出</button>
            <button type="button" className={form.kind === "income" ? "is-active is-income" : ""} onClick={() => onKindChange("income")}><ArrowDownRight size={17} />收入</button>
          </div>
          <label className="ledger-field ledger-field--amount">
            <span>金额</span>
            <div><strong>{currency}</strong><input autoFocus inputMode="decimal" type="text" value={form.amount} onChange={(event) => onChange({ ...form, amount: sanitizeAmountInput(event.target.value) })} placeholder="0.00" aria-label="金额" /></div>
          </label>
          <div className="ledger-form-grid">
            <label className="ledger-field"><span>日期</span><input type="date" required value={form.date} onChange={(event) => onChange({ ...form, date: event.target.value })} /></label>
            <label className="ledger-field"><span>分类</span><select value={form.category} onChange={(event) => onChange({ ...form, category: event.target.value })}>{categories.map((category) => <option key={category} value={category}>{category}</option>)}</select></label>
            <label className="ledger-field"><span>账户</span><select value={form.account} onChange={(event) => onChange({ ...form, account: event.target.value })}>{DEFAULT_ACCOUNTS.map((account) => <option key={account} value={account}>{account}</option>)}</select></label>
            <label className="ledger-field ledger-field--note"><span>备注</span><input type="text" maxLength={500} value={form.note} onChange={(event) => onChange({ ...form, note: event.target.value })} placeholder="可选，例如早餐、房租" /></label>
          </div>
          {error ? <div className="ledger-form-error" role="alert"><CircleAlert size={16} />{error}</div> : null}
          <div className="ledger-dialog-actions">
            <button type="button" className="ledger-secondary-action" onClick={onClose} disabled={saving}>取消</button>
            <button type="button" className="ledger-primary-action" disabled={saving} onClick={() => void onSubmit()}>
              {saving ? <LoaderCircle size={16} className="ledger-spin" /> : <Check size={16} />}
              {saving ? "保存中" : "保存账单"}
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

function LedgerEmpty({ title, detail, onCreate }: { title: string; detail: string; onCreate: () => void }) {
  return (
    <div className="ledger-empty">
      <span aria-hidden="true"><ReceiptText size={24} /></span>
      <strong>{title}</strong>
      <p>{detail}</p>
      <button type="button" className="ledger-secondary-action" onClick={onCreate}><Plus size={15} />记一笔</button>
    </div>
  );
}

function LedgerLoading() {
  return (
    <div className="ledger-loading" role="status">
      <LoaderCircle size={20} className="ledger-spin" />
      <span>正在读取账本...</span>
    </div>
  );
}

function readStringProp(node: GameUiComponentNode | undefined, key: string, fallback: string): string {
  const value = node?.props?.[key];
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}

function readCollectionProp(node: GameUiComponentNode | undefined, key: string, fallback: string): string {
  const value = readStringProp(node, key, fallback).toLocaleLowerCase();
  return /^[a-z0-9._-]{1,64}$/.test(value) ? value : fallback;
}

function readStringListProp(node: GameUiComponentNode | undefined, key: string, fallback: string[]): string[] {
  const value = node?.props?.[key];
  if (!Array.isArray(value)) {
    return fallback;
  }
  const normalized = value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim().slice(0, 80))
    .filter(Boolean)
    .slice(0, 40);
  return normalized.length > 0 ? Array.from(new Set(normalized)) : fallback;
}

function readLedgerView(value: unknown): LedgerView {
  return value === "transactions" || value === "stats" ? value : "overview";
}

function decodeLedgerRecords(value: unknown): { entries: LedgerEntry[]; invalidCount: number } {
  if (!Array.isArray(value)) {
    throw new Error("宿主返回的账本列表格式不正确。");
  }
  const entries: LedgerEntry[] = [];
  let invalidCount = 0;
  for (const item of value) {
    const entry = decodeLedgerRecord(item);
    if (entry) {
      entries.push(entry);
    } else {
      invalidCount += 1;
    }
  }
  return { entries, invalidCount };
}

export function decodeLedgerRecord(value: unknown): LedgerEntry | null {
  if (!isRecord(value) || typeof value.id !== "string" || !isRecord(value.data)) {
    return null;
  }
  const data = value.data;
  const kind = data.kind;
  const amountCents = Number(data.amount_cents);
  const date = typeof data.date === "string" ? data.date : "";
  if (
    (kind !== "income" && kind !== "expense")
    || !Number.isSafeInteger(amountCents)
    || amountCents <= 0
    || !isValidIsoDate(date)
  ) {
    return null;
  }
  return {
    id: value.id,
    kind,
    amountCents,
    date,
    category: readRecordString(data.category, "未分类", 80),
    account: readRecordString(data.account, "", 80),
    note: readRecordString(data.note, "", 500),
    createdAt: typeof value.created_at === "string" ? value.created_at : "",
    updatedAt: typeof value.updated_at === "string" ? value.updated_at : "",
  };
}

function readRecordString(value: unknown, fallback: string, maxLength: number): string {
  return typeof value === "string" && value.trim() ? value.trim().slice(0, maxLength) : fallback;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function sortEntries(entries: LedgerEntry[]): LedgerEntry[] {
  return [...entries].sort((left, right) => {
    const dateOrder = right.date.localeCompare(left.date);
    if (dateOrder !== 0) {
      return dateOrder;
    }
    return (right.createdAt || right.updatedAt).localeCompare(left.createdAt || left.updatedAt);
  });
}

export function calculateTotals(entries: LedgerEntry[]): LedgerTotals {
  let income = 0;
  let expense = 0;
  for (const entry of entries) {
    if (entry.kind === "income") {
      income += entry.amountCents;
    } else {
      expense += entry.amountCents;
    }
  }
  return { income, expense, balance: income - expense };
}

function buildCategoryBreakdown(entries: LedgerEntry[], kind: LedgerKind) {
  const totals = new Map<string, number>();
  for (const entry of entries) {
    if (entry.kind === kind) {
      totals.set(entry.category, (totals.get(entry.category) ?? 0) + entry.amountCents);
    }
  }
  return Array.from(totals, ([category, amount]) => ({ category, amount }))
    .sort((left, right) => right.amount - left.amount);
}

function buildTrend(entries: LedgerEntry[], mode: PeriodMode, value: string): TrendBucket[] {
  let keys: Array<{ key: string; label: string }>;
  if (mode === "month") {
    const [year, month] = value.split("-").map(Number);
    const dayCount = Number.isFinite(year) && Number.isFinite(month)
      ? new Date(year, month, 0).getDate()
      : 31;
    keys = Array.from({ length: dayCount }, (_, index) => {
      const day = String(index + 1).padStart(2, "0");
      return { key: `${value}-${day}`, label: String(index + 1) };
    });
  } else if (mode === "year") {
    keys = Array.from({ length: 12 }, (_, index) => {
      const month = String(index + 1).padStart(2, "0");
      return { key: `${value}-${month}`, label: `${index + 1}月` };
    });
  } else if (mode === "day") {
    const categories = Array.from(new Set(entries.map((entry) => entry.category))).slice(0, 12);
    keys = categories.map((category) => ({ key: category, label: category }));
  } else {
    const years = Array.from(new Set(entries.map((entry) => entry.date.slice(0, 4)))).sort();
    keys = years.map((year) => ({ key: year, label: year }));
  }

  const buckets = new Map(keys.map((item) => [item.key, { ...item, income: 0, expense: 0 }]));
  for (const entry of entries) {
    const key = mode === "month"
      ? entry.date
      : mode === "year"
        ? entry.date.slice(0, 7)
        : mode === "day"
          ? entry.category
          : entry.date.slice(0, 4);
    const bucket = buckets.get(key);
    if (bucket) {
      bucket[entry.kind] += entry.amountCents;
    }
  }
  return Array.from(buckets.values());
}

function groupEntriesByDate(entries: LedgerEntry[]) {
  const groups = new Map<string, LedgerEntry[]>();
  for (const entry of entries) {
    const current = groups.get(entry.date) ?? [];
    current.push(entry);
    groups.set(entry.date, current);
  }
  return Array.from(groups, ([date, groupedEntries]) => ({ date, entries: groupedEntries }));
}

export function matchesPeriod(date: string, mode: PeriodMode, value: string): boolean {
  if (mode === "all") {
    return true;
  }
  if (mode === "day") {
    return date === value;
  }
  if (mode === "month") {
    return date.startsWith(`${value}-`);
  }
  return date.startsWith(`${value}-`);
}

function currentPeriodValue(mode: PeriodMode): string {
  const today = todayIsoDate();
  if (mode === "day") {
    return today;
  }
  if (mode === "month") {
    return today.slice(0, 7);
  }
  if (mode === "year") {
    return today.slice(0, 4);
  }
  return "all";
}

function stepPeriod(value: string, mode: PeriodMode, direction: -1 | 1): string {
  if (mode === "day" && isValidIsoDate(value)) {
    const [year, month, day] = value.split("-").map(Number);
    const date = new Date(year, month - 1, day + direction, 12);
    return localIsoDate(date);
  }
  if (mode === "month" && /^\d{4}-\d{2}$/.test(value)) {
    const [year, month] = value.split("-").map(Number);
    const date = new Date(year, month - 1 + direction, 1, 12);
    return localIsoDate(date).slice(0, 7);
  }
  if (mode === "year") {
    const year = Number(value);
    return String(Math.min(2200, Math.max(1900, (Number.isFinite(year) ? year : new Date().getFullYear()) + direction)));
  }
  return value;
}

function normalizeYear(value: string): string {
  const digits = value.replace(/\D/g, "").slice(0, 4);
  return digits || String(new Date().getFullYear());
}

function formatPeriodLabel(mode: PeriodMode, value: string): string {
  if (mode === "all") {
    return "全部时间";
  }
  if (mode === "year") {
    return `${value} 年`;
  }
  if (mode === "month") {
    const [year, month] = value.split("-");
    return `${year} 年 ${Number(month)} 月`;
  }
  return formatFriendlyDate(value);
}

function formatFriendlyDate(value: string): string {
  if (!isValidIsoDate(value)) {
    return value;
  }
  const [year, month, day] = value.split("-").map(Number);
  return `${year}年${month}月${day}日`;
}

function weekdayLabel(value: string): string {
  if (!isValidIsoDate(value)) {
    return "";
  }
  const [year, month, day] = value.split("-").map(Number);
  return ["周日", "周一", "周二", "周三", "周四", "周五", "周六"][new Date(year, month - 1, day, 12).getDay()];
}

function formatDayNet(entries: LedgerEntry[], currency: string): string {
  const totals = calculateTotals(entries);
  return `收 ${formatMoney(totals.income, currency)} · 支 ${formatMoney(totals.expense, currency)}`;
}

function formatMoney(cents: number, currency: string): string {
  const amount = Math.abs(cents) / 100;
  return `${currency}${amount.toLocaleString("zh-CN", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

function formatSignedMoney(cents: number, currency: string): string {
  return `${cents >= 0 ? "+" : "-"}${formatMoney(cents, currency)}`;
}

export function parseAmountToCents(value: string): number | null {
  const normalized = value.trim();
  if (!/^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/.test(normalized)) {
    return null;
  }
  const [whole, fraction = ""] = normalized.split(".");
  const cents = Number(whole) * 100 + Number(fraction.padEnd(2, "0"));
  return Number.isSafeInteger(cents) && cents > 0 ? cents : null;
}

function centsToInput(cents: number): string {
  return (cents / 100).toFixed(2);
}

function sanitizeAmountInput(value: string): string {
  const normalized = value.replace(/[^\d.]/g, "");
  const [whole = "", ...rest] = normalized.split(".");
  const fraction = rest.join("").slice(0, 2);
  return rest.length > 0 ? `${whole.slice(0, 12)}.${fraction}` : whole.slice(0, 12);
}

function todayIsoDate(): string {
  return localIsoDate(new Date());
}

function localIsoDate(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function isValidIsoDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    return false;
  }
  const [year, month, day] = value.split("-").map(Number);
  const date = new Date(year, month - 1, day, 12);
  return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day;
}

function readErrorMessage(errorLike: unknown, fallback: string): string {
  if (errorLike instanceof Error && errorLike.message.trim()) {
    return errorLike.message;
  }
  if (typeof errorLike === "string" && errorLike.trim()) {
    return errorLike;
  }
  return fallback;
}

export const LEDGER_PREVIEW_RECORDS: StoredWorldRecord[] = [
  {
    id: "6f12196a-a6e2-4d9e-9103-b9e328092b89",
    world_id: "preview-world",
    collection: "ledger.entries",
    data: { schema_version: 1, kind: "income", amount_cents: 1280000, date: todayIsoDate(), category: "工资", account: "银行卡", note: "本月工资" },
    created_at: `${todayIsoDate()}T09:00:00.000Z`,
    updated_at: `${todayIsoDate()}T09:00:00.000Z`,
  },
  {
    id: "f47708f8-ed7d-4d11-8f53-cc573bce9f20",
    world_id: "preview-world",
    collection: "ledger.entries",
    data: { schema_version: 1, kind: "expense", amount_cents: 3680, date: todayIsoDate(), category: "餐饮", account: "微信", note: "午餐" },
    created_at: `${todayIsoDate()}T05:30:00.000Z`,
    updated_at: `${todayIsoDate()}T05:30:00.000Z`,
  },
  {
    id: "ee87fcb0-cff4-46fb-8520-506bbad0bf98",
    world_id: "preview-world",
    collection: "ledger.entries",
    data: { schema_version: 1, kind: "expense", amount_cents: 1350, date: todayIsoDate(), category: "交通", account: "支付宝", note: "地铁" },
    created_at: `${todayIsoDate()}T01:15:00.000Z`,
    updated_at: `${todayIsoDate()}T01:15:00.000Z`,
  },
];
