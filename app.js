(() => {
  const STORAGE_KEY = "healthy-life-attrs-v1";

  const titles = { chat: "对话", growth: "成长", profile: "我的" };
  const navItems = Array.from(document.querySelectorAll(".side-item, .tab"));
  const panes = Array.from(document.querySelectorAll(".pane"));
  const pageTitle = document.getElementById("pageTitle");
  const chatInput = document.getElementById("chatInput");
  const chatScroll = document.getElementById("chatScroll");
  const sendBtn = document.getElementById("sendBtn");

  /** 用户自填的自定义属性（对话时带给模型） */
  const defaults = {
    height_cm: 0,
    weight_kg: 0,
    age: 0,
    gender: "未填写",
    goal_type: "减重",
    target_weight: 0,
    target_height: 0,
    intake_kcal: 0,
    burn_kcal: 0,
  };

  function loadAttrs() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return { ...defaults };
      return { ...defaults, ...JSON.parse(raw) };
    } catch {
      return { ...defaults };
    }
  }

  function saveAttrs(next) {
    Object.assign(attrs, next);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(attrs));
    renderAll();
  }

  const attrs = loadAttrs();

  function switchTab(name) {
    navItems.forEach((el) => el.classList.toggle("is-active", el.dataset.tab === name));
    panes.forEach((pane) => pane.classList.toggle("is-active", pane.dataset.tab === name));
    if (pageTitle) pageTitle.textContent = titles[name] || "对话";
  }

  navItems.forEach((el) => el.addEventListener("click", () => switchTab(el.dataset.tab)));

  function calcBmi() {
    const m = attrs.height_cm / 100;
    if (!(m > 0) || !(attrs.weight_kg > 0)) return null;
    const bmi = Math.round((attrs.weight_kg / (m * m)) * 10) / 10;
    let label = "正常";
    if (bmi < 18.5) label = "偏瘦";
    else if (bmi < 24) label = "正常";
    else if (bmi < 28) label = "超重";
    else label = "肥胖";
    return { bmi, label };
  }

  /** Mifflin-St Jeor；性别未填则不给数 */
  function calcBmr() {
    const { height_cm: h, weight_kg: w, age, gender } = attrs;
    if (!(h > 0) || !(w > 0) || !(age > 0) || (gender !== "男" && gender !== "女")) return null;
    const base = 10 * w + 6.25 * h - 5 * age;
    return Math.round(gender === "女" ? base - 161 : base + 5);
  }

  function energyNet() {
    return (Number(attrs.intake_kcal) || 0) - (Number(attrs.burn_kcal) || 0);
  }

  /** 对话时带给模型的属性快照 */
  function attributeSnapshot() {
    const bmi = calcBmi();
    const bmr = calcBmr();
    const net = energyNet();
    return {
      health_height_cm: attrs.height_cm || null,
      health_weight_kg: attrs.weight_kg || null,
      health_age: attrs.age || null,
      health_gender: attrs.gender,
      health_bmi: bmi ? bmi.bmi : null,
      health_bmi_label: bmi ? bmi.label : "待测量",
      health_bmr: bmr,
      health_goal_type: attrs.goal_type,
      health_target_weight: attrs.target_weight || null,
      health_target_height: attrs.target_height || null,
      health_intake_kcal: attrs.intake_kcal,
      health_burn_kcal: attrs.burn_kcal,
      health_energy_net: net,
    };
  }

  function formatAttrsLines() {
    const snap = attributeSnapshot();
    return Object.entries(snap)
      .map(([k, v]) => `${k}: ${v === null || v === "" ? "未填写" : v}`)
      .join("\n");
  }

  function fillForms() {
    const set = (id, val) => {
      const el = document.getElementById(id);
      if (el) el.value = val === 0 || val === "未填写" ? (el.tagName === "SELECT" ? val : "") : val;
    };
    set("height", attrs.height_cm || "");
    set("weight", attrs.weight_kg || "");
    set("age", attrs.age || "");
    set("gender", attrs.gender || "未填写");
    set("goalType", attrs.goal_type || "减重");
    set("targetWeight", attrs.target_weight || "");
    set("targetHeight", attrs.target_height || "");
    set("intake", attrs.intake_kcal || "");
    set("burn", attrs.burn_kcal || "");
  }

  function renderDerived() {
    const bmi = calcBmi();
    const bmr = calcBmr();
    const net = energyNet();

    const bmiNum = document.getElementById("bmiNum");
    const bmiTag = document.getElementById("bmiTag");
    const bmrNum = document.getElementById("bmrNum");
    if (bmiNum) bmiNum.textContent = bmi ? String(bmi.bmi) : "--";
    if (bmiTag) bmiTag.textContent = bmi ? bmi.label : "待填写";
    if (bmrNum) bmrNum.textContent = bmr == null ? "--" : String(bmr);

    const goalTitle = document.getElementById("goalTitle");
    const goalSub = document.getElementById("goalSub");
    if (goalTitle) goalTitle.textContent = attrs.goal_type || "未设定";
    if (goalSub) {
      const parts = [];
      if (attrs.weight_kg > 0 && attrs.target_weight > 0) {
        parts.push(`${attrs.weight_kg} → ${attrs.target_weight} kg`);
      } else if (attrs.target_weight > 0) {
        parts.push(`目标 ${attrs.target_weight} kg`);
      }
      if (attrs.target_height > 0) parts.push(`目标身高 ${attrs.target_height} cm`);
      goalSub.textContent = parts.join(" · ") || "在下方填写目标";
    }

    const ringFill = document.getElementById("ringFill");
    const netEl = document.getElementById("netEnergy");
    if (netEl) {
      netEl.textContent = (net > 0 ? "+" : "") + String(net);
      netEl.classList.toggle("is-negative", net < 0);
      netEl.classList.toggle("is-positive", net > 0);
    }
    if (ringFill) {
      const mag = Math.min(Math.abs(net), 800);
      const deg = net === 0 ? 0 : Math.round((mag / 800) * 180);
      ringFill.style.setProperty("--ring-deg", `${deg}deg`);
      ringFill.classList.toggle("is-positive", net > 0);
      ringFill.classList.toggle("is-negative", net < 0);
    }

    const text = formatAttrsLines();
    ["attrPreview", "dockAttrs"].forEach((id) => {
      const el = document.getElementById(id);
      if (el) el.textContent = text;
    });

    const strip = document.getElementById("attrStrip");
    if (strip) {
      const bmiLabel = bmi ? `BMI ${bmi.bmi} ${bmi.label}` : "BMI 待填";
      const bmrLabel = bmr == null ? "BMR 待填" : `BMR ${bmr}`;
      strip.textContent = `属性 · ${attrs.gender} ${attrs.height_cm || "--"}cm ${attrs.weight_kg || "--"}kg · ${bmiLabel} · ${bmrLabel} · 摄入${attrs.intake_kcal}/消耗${attrs.burn_kcal} · 目标${attrs.goal_type}`;
    }
  }

  function renderAll() {
    fillForms();
    renderDerived();
  }

  function num(id, fallback = 0) {
    const el = document.getElementById(id);
    if (!el) return fallback;
    const v = Number(el.value);
    return Number.isFinite(v) ? v : fallback;
  }

  document.getElementById("saveProfile")?.addEventListener("click", () => {
    saveAttrs({
      height_cm: num("height"),
      weight_kg: num("weight"),
      age: num("age"),
      gender: document.getElementById("gender")?.value || "未填写",
    });
    appendBubble("agent", "档案已保存。BMI / BMR 已更新，对话会自动带上这些属性。");
  });

  document.getElementById("saveGoal")?.addEventListener("click", () => {
    saveAttrs({
      goal_type: document.getElementById("goalType")?.value || "减重",
      target_weight: num("targetWeight"),
      target_height: num("targetHeight"),
    });
    appendBubble("agent", "目标已保存，会作为自定义属性带给模型。");
  });

  document.getElementById("saveEnergy")?.addEventListener("click", () => {
    saveAttrs({
      intake_kcal: num("intake"),
      burn_kcal: num("burn"),
    });
    appendBubble("agent", `今日能量已保存：摄入 ${attrs.intake_kcal} / 消耗 ${attrs.burn_kcal}，净 ${energyNet()} kcal。`);
  });

  document.querySelectorAll("[data-bump]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const key = btn.getAttribute("data-bump");
      const delta = Number(btn.getAttribute("data-delta") || 0);
      if (key !== "intake" && key !== "burn") return;
      const next = { [key]: Math.max(0, (Number(attrs[key]) || 0) + delta) };
      saveAttrs(next);
    });
  });

  renderAll();

  function appendBubble(role, text) {
    if (!chatScroll) return;
    const article = document.createElement("article");
    article.className = `bubble bubble--${role}`;
    const p = document.createElement("p");
    p.textContent = text;
    article.appendChild(p);
    const quick = chatScroll.querySelector(".quick-row");
    if (quick) chatScroll.insertBefore(article, quick);
    else chatScroll.appendChild(article);
    chatScroll.scrollTop = chatScroll.scrollHeight;
  }

  /** 发送时附带自定义属性，模拟 runtime_attributes 注入 */
  function sendText(raw) {
    const text = String(raw || "").trim();
    if (!text) return;
    switchTab("chat");
    appendBubble("player", text);
    const snap = attributeSnapshot();
    window.setTimeout(() => {
      const lines = [
        "已读取你的自定义属性（无需你再报数）：",
        `· ${snap.health_gender} · ${snap.health_height_cm ?? "--"}cm · ${snap.health_weight_kg ?? "--"}kg`,
        `· BMI ${snap.health_bmi ?? "--"} ${snap.health_bmi_label} · BMR ${snap.health_bmr ?? "--"}`,
        `· 目标 ${snap.health_goal_type} · 今日 ${snap.health_intake_kcal}/${snap.health_burn_kcal} · 净 ${snap.health_energy_net}`,
        "",
        "基于以上给建议。改数据请到「成长 / 我的」自己填，我只读不代填。",
      ];
      appendBubble("agent", lines.join("\n"));
    }, 320);
  }

  if (sendBtn) {
    sendBtn.addEventListener("click", () => {
      sendText(chatInput && chatInput.value);
      if (chatInput) {
        chatInput.value = "";
        chatInput.style.height = "auto";
      }
    });
  }

  if (chatInput) {
    chatInput.addEventListener("input", () => {
      chatInput.style.height = "auto";
      chatInput.style.height = `${Math.min(chatInput.scrollHeight, 120)}px`;
    });
    chatInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        sendBtn && sendBtn.click();
      }
    });
  }

  document.querySelectorAll("[data-send]").forEach((btn) => {
    btn.addEventListener("click", () => sendText(btn.getAttribute("data-send") || ""));
  });
})();
