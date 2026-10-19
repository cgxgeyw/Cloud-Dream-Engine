(function () {
  var week = [
    { label: "20", burn: 1920 },
    { label: "21", burn: 1850 },
    { label: "22", burn: 2050 },
    { label: "23", burn: 1780 },
    { label: "24", burn: 2010 },
    { label: "25", burn: 1880 },
    { label: "26", burn: 1940, selected: true }
  ];
  var month = [];
  for (var i = 1; i <= 28; i++) {
    month.push({
      label: String(i),
      burn: i % 3 !== 0 ? 1600 + ((i * 37) % 500) : 0,
      selected: i === 26
    });
  }
  var days = [
    { date: "2026-07-26", intake: 1800, burn: 1920, net: -120 },
    { date: "2026-07-25", intake: 2100, burn: 1850, net: 250 },
    { date: "2026-07-24", intake: 1650, burn: 2050, net: -400 },
    { date: "2026-07-23", intake: 2200, burn: 1780, net: 420 },
    { date: "2026-07-22", intake: 1720, burn: 2010, net: -290 }
  ];

  var tabs = document.querySelectorAll(".tab");
  var panes = document.querySelectorAll("[data-pane]");
  tabs.forEach(function (tab) {
    tab.addEventListener("click", function () {
      var id = tab.getAttribute("data-tab");
      tabs.forEach(function (t) { t.classList.toggle("is-active", t === tab); });
      panes.forEach(function (p) {
        p.classList.toggle("is-active", p.getAttribute("data-pane") === id);
      });
    });
  });

  var modeBtns = document.querySelectorAll(".mode-btn");
  var barsEl = document.getElementById("bars");
  var dayListEl = document.getElementById("dayList");

  function renderBars(data) {
    barsEl.hidden = false;
    dayListEl.hidden = true;
    var selectedIdx = data.findIndex(function (item) { return item.selected; });
    if (selectedIdx < 0) selectedIdx = data.length - 1;
    barsEl.innerHTML = data.map(function (item, idx) {
      var on = idx === selectedIdx;
      var h = item.burn || item.heightNum || 0;
      var max = Math.max.apply(null, data.map(function (d) { return d.burn || d.heightNum || 0; }).concat([1]));
      var pct = Math.round((h / max) * 100);
      return (
        '<div class="bar-col">' +
        (on ? '<div class="bar-value">' + (item.burn_text || (h + " kcal")) + "</div>" : '<div class="bar-value" style="visibility:hidden">·</div>') +
        '<div class="bar-pair">' +
        '<div class="bar ' + (on ? "bar--on" : "bar--off") + '" style="height:' + pct + '%"></div>' +
        "</div>" +
        '<div class="bar-label' + (on ? " bar-label--selected" : "") + '">' + item.label + "</div>" +
        "</div>"
      );
    }).join("");
  }

  function renderDays() {
    barsEl.hidden = true;
    dayListEl.hidden = false;
    dayListEl.innerHTML = days.map(function (d) {
      return (
        '<div class="day-row">' +
        '<span class="day-date">' + d.date + "</span>" +
        '<span class="in">摄 ' + d.intake + "</span>" +
        '<span class="out">耗 ' + d.burn + "</span>" +
        "<span>净 " + d.net + "</span>" +
        "</div>"
      );
    }).join("");
  }

  modeBtns.forEach(function (btn) {
    btn.addEventListener("click", function () {
      var mode = btn.getAttribute("data-mode");
      modeBtns.forEach(function (b) { b.classList.toggle("is-active", b === btn); });
      if (mode === "week") renderBars(week);
      else if (mode === "month") renderBars(month);
      else renderDays();
    });
  });

  renderBars(week);
})();
