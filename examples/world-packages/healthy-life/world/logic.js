world.register("ui.setTab", async (input) => {
  const map = { growth: "records", mine: "profile" };
  const allowed = ["chat", "records", "plan", "profile"];
  const raw = input && input.tab != null ? String(input.tab) : "chat";
  const tab = map[raw] || raw;
  return allowed.includes(tab) ? tab : "chat";
});

function num(v) {
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

function pad2(n) {
  return String(n).padStart(2, "0");
}

function formatDate(d) {
  return d.getFullYear() + "-" + pad2(d.getMonth() + 1) + "-" + pad2(d.getDate());
}

function daysInMonth(y, m) {
  return new Date(y, m, 0).getDate();
}

function emptyDay(date) {
  return { id: "", date: date, intake: 0, burn: 0, net: 0, bmr: 0, steps: 0, note: "" };
}

/**
 * 今日总消耗 = BMR + 运动。模型有时只写运动量或漏写，这里做确定性兜底：
 * - burn<=0 且已知 BMR → 至少计基础代谢
 * - 0 < burn < BMR → 视为只写了运动量，总消耗 = BMR + 运动
 * - burn >= BMR → 视为已是总消耗，原样使用
 */
function resolveBurnWithBmr(burn, bmr) {
  if (bmr <= 0) return burn;
  if (burn <= 0) return bmr;
  if (burn < bmr) return bmr + burn;
  return burn;
}

function recordId(row) {
  return String((row && (row.id || row.record_id || row.recordId)) || "");
}

function rowToDay(row) {
  const data = row && row.data && typeof row.data === "object" ? row.data : row || {};
  const date = String(data.date || "");
  if (!date) return null;
  const intake = num(data.intake);
  const burn = num(data.burn);
  const net = Number.isFinite(Number(data.net)) ? Number(data.net) : intake - burn;
  return {
    id: recordId(row),
    date: date,
    intake: intake,
    burn: burn,
    net: net,
    bmr: num(data.bmr),
    steps: num(data.steps),
    note: String(data.note || "")
  };
}

function maxCalOf(days) {
  let max = 1;
  for (let i = 0; i < days.length; i++) {
    const d = days[i];
    if (d.intake > max) max = d.intake;
    if (d.burn > max) max = d.burn;
  }
  return max;
}

function barFromDay(day, maxCal, selected) {
  const max = maxCal > 0 ? maxCal : 1;
  const hIn = Math.round(Math.min(100, (day.intake / max) * 100));
  const hBurn = Math.round(Math.min(100, (day.burn / max) * 100));
  const label = day.date ? day.date.slice(8) : "--";
  return {
    date: day.date,
    label: label,
    intake: day.intake,
    burn: day.burn,
    net: day.net,
    height: hBurn + "%",
    burn_text: day.burn > 0 ? day.burn + " kcal" : "—",
    selected: !!selected,
    h_in: hIn + "%",
    h_burn: hBurn + "%",
    short: label + "日"
  };
}

function average(days, key) {
  if (!days.length) return 0;
  let sum = 0;
  for (let i = 0; i < days.length; i++) sum += num(days[i][key]);
  return Math.round(sum / days.length);
}

async function listEnergyDays(api) {
  const raw = await api.records.list("health.energy_days");
  const rows = Array.isArray(raw) ? raw : (raw && raw.records) || [];
  const days = [];
  for (let i = 0; i < rows.length; i++) {
    const day = rowToDay(rows[i]);
    if (day) days.push(day);
  }
  days.sort(function (a, b) {
    return a.date < b.date ? 1 : a.date > b.date ? -1 : 0;
  });
  return days;
}

function buildStats(days, today, selectedDate) {
  const todayStr = formatDate(new Date());
  const now = new Date();
  const y = now.getFullYear();
  const m = now.getMonth() + 1;
  const byDate = {};
  for (let i = 0; i < days.length; i++) byDate[days[i].date] = days[i];

  const weekDays = [];
  for (let i = 6; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
    const key = formatDate(d);
    weekDays.push(byDate[key] || emptyDay(key));
  }

  const dim = daysInMonth(y, m);
  const monthDays = [];
  for (let i = 1; i <= dim; i++) {
    const key = y + "-" + pad2(m) + "-" + pad2(i);
    monthDays.push(byDate[key] || emptyDay(key));
  }

  const weekMax = maxCalOf(weekDays);
  const monthMax = maxCalOf(monthDays);
  const selectedKey0 = selectedDate || today.date || todayStr;
  const weekBars = weekDays.map(function (d) {
    return barFromDay(d, weekMax, d.date === selectedKey0);
  });
  const monthBars = monthDays.map(function (d) {
    return barFromDay(d, monthMax, d.date === selectedKey0);
  });

  const dayList = days.slice(0, 40).map(function (d) {
    return {
      date: d.date,
      label: d.date,
      intake: d.intake,
      burn: d.burn,
      net: d.net,
      steps: d.steps,
      is_today: d.date === todayStr
    };
  });

  const selectedKey = selectedDate || today.date || todayStr;
  const selected =
    byDate[selectedKey] ||
    days.find(function (d) {
      return d.date === selectedKey;
    }) ||
    (selectedKey === today.date ? today : emptyDay(selectedKey));

  const monthActive = monthDays.filter(function (d) {
    return d.intake > 0 || d.burn > 0;
  });

  return {
    generated_at: todayStr,
    today: today,
    week_bars: weekBars,
    month_bars: monthBars,
    day_list: dayList,
    week_avg: {
      intake: average(weekDays, "intake"),
      burn: average(weekDays, "burn"),
      net: average(weekDays, "net")
    },
    month_avg: {
      intake: average(monthActive, "intake"),
      burn: average(monthActive, "burn"),
      net: average(monthActive, "net")
    },
    recorded_days: days.length,
    selected: selected
  };
}

function readInputRecord(input) {
  return input && typeof input === "object" ? input : {};
}

async function loadStatsCore(input, api) {
  const source = readInputRecord(input);
  const days = await listEnergyDays(api);
  const todayStr = formatDate(new Date());
  let today = null;
  for (let i = 0; i < days.length; i++) {
    if (days[i].date === todayStr) {
      today = days[i];
      break;
    }
  }
  if (!today) {
    const bmr = num(source.bmr);
    const intake = num(source.intake);
    const burn = resolveBurnWithBmr(num(source.burn), bmr);
    const net = Number.isFinite(Number(source.net)) ? num(source.net) : intake - burn;
    today = {
      id: "",
      date: todayStr,
      intake: intake,
      burn: burn,
      net: net,
      bmr: bmr,
      steps: num(source.steps),
      note: ""
    };
  }
  const selectedDate =
    typeof source.selected_date === "string" && source.selected_date
      ? source.selected_date
      : today.date;
  return buildStats(days, today, selectedDate);
}

world.register("health.syncStats", async (input, api) => {
  const source = readInputRecord(input);
  const date = formatDate(new Date());
  const intake = num(source.intake);
  const bmr = num(source.bmr);
  const burn = resolveBurnWithBmr(num(source.burn), bmr);
  const steps = num(source.steps);
  const net = Number.isFinite(Number(source.net)) ? num(source.net) : intake - burn;
  const payload = {
    date: date,
    intake: intake,
    burn: burn,
    net: net,
    bmr: bmr,
    steps: steps,
    note: ""
  };

  const existingDays = await listEnergyDays(api);
  let hit = null;
  for (let i = 0; i < existingDays.length; i++) {
    if (existingDays[i].date === date) {
      hit = existingDays[i];
      break;
    }
  }
  if (hit && hit.id) {
    await api.records.update("health.energy_days", hit.id, payload);
  } else {
    await api.records.create("health.energy_days", payload);
  }

  const days = await listEnergyDays(api);
  let today = null;
  for (let i = 0; i < days.length; i++) {
    if (days[i].date === date) {
      today = days[i];
      break;
    }
  }
  if (!today) {
    today = Object.assign(emptyDay(date), payload, { id: (hit && hit.id) || "" });
  }
  const selectedDate =
    typeof source.selected_date === "string" && source.selected_date ? source.selected_date : date;
  return buildStats(days, today, selectedDate);
});

world.register("health.loadStats", async (input, api) => {
  return loadStatsCore(input, api);
});

world.register("health.selectDay", async (input, api) => {
  const source = readInputRecord(input);
  const selectedDate =
    String(source.date || source.selected_date || "") || formatDate(new Date());
  return loadStatsCore(
    Object.assign({}, source, { selected_date: selectedDate }),
    api
  );
});
