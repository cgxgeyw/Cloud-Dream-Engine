import { describe, expect, it } from "vitest";

import {
  calculateTotals,
  decodeLedgerRecord,
  matchesPeriod,
  parseAmountToCents,
} from "./LedgerBook";

function storedRecord(id: string, kind: "income" | "expense", amountCents: number, date: string) {
  return {
    id,
    world_id: "world-a",
    collection: "ledger.entries",
    data: {
      schema_version: 1,
      kind,
      amount_cents: amountCents,
      date,
      category: kind === "income" ? "工资" : "餐饮",
      account: "银行卡",
      note: "",
    },
    created_at: `${date}T09:00:00.000Z`,
    updated_at: `${date}T09:00:00.000Z`,
  };
}

describe("ledger amount handling", () => {
  it("converts decimal input to integer cents without floating point arithmetic", () => {
    expect(parseAmountToCents("12.34")).toBe(1234);
    expect(parseAmountToCents("0.01")).toBe(1);
    expect(parseAmountToCents("120")).toBe(12000);
  });

  it("rejects zero, negative, over-precision and unsafe amounts", () => {
    expect(parseAmountToCents("0")).toBeNull();
    expect(parseAmountToCents("-1")).toBeNull();
    expect(parseAmountToCents("12.345")).toBeNull();
    expect(parseAmountToCents("999999999999999")).toBeNull();
  });
});

describe("ledger record decoding and totals", () => {
  it("accepts the versioned ledger shape and sums income and expense", () => {
    const income = decodeLedgerRecord(storedRecord(
      "6f12196a-a6e2-4d9e-9103-b9e328092b89",
      "income",
      120000,
      "2026-07-24",
    ));
    const expense = decodeLedgerRecord(storedRecord(
      "f47708f8-ed7d-4d11-8f53-cc573bce9f20",
      "expense",
      3580,
      "2026-07-24",
    ));

    expect(income).not.toBeNull();
    expect(expense).not.toBeNull();
    expect(calculateTotals([income!, expense!])).toEqual({
      income: 120000,
      expense: 3580,
      balance: 116420,
    });
  });

  it("rejects floating amounts and impossible dates from stored data", () => {
    expect(decodeLedgerRecord(storedRecord(
      "6f12196a-a6e2-4d9e-9103-b9e328092b89",
      "income",
      12.5,
      "2026-07-24",
    ))).toBeNull();
    expect(decodeLedgerRecord(storedRecord(
      "6f12196a-a6e2-4d9e-9103-b9e328092b89",
      "income",
      1250,
      "2026-02-30",
    ))).toBeNull();
  });
});

describe("ledger period matching", () => {
  it("matches day, month, year and all-time filters", () => {
    expect(matchesPeriod("2026-07-24", "day", "2026-07-24")).toBe(true);
    expect(matchesPeriod("2026-07-24", "day", "2026-07-23")).toBe(false);
    expect(matchesPeriod("2026-07-24", "month", "2026-07")).toBe(true);
    expect(matchesPeriod("2026-07-24", "year", "2026")).toBe(true);
    expect(matchesPeriod("2026-07-24", "all", "all")).toBe(true);
  });
});
