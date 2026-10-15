/* Local report rendering. No network, storage, HTML injection or client-side labor calculations. */
"use strict";
(() => {
  const $ = (id) => document.getElementById(id);
  const el = (tag, text, className) => {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  };
  const report = JSON.parse($("report-data").textContent);
  const employees = report.employees;
  const attention = (employee) => Boolean(employee.refused || employee.break_shortages.length || employee.ambiguities.length);
  const calculated = employees.filter((employee) => !employee.refused && employee.ledger.length);
  const refused = employees.filter((employee) => employee.refused);
  const flagged = employees.filter(attention);
  const duration = (minutes) => `${Math.floor(minutes / 60).toLocaleString("ja-JP")}時間${minutes % 60}分`;
  const sum = (key) => calculated.reduce((total, employee) => total + employee.ledger[0][key], 0);
  const status = (employee) => employee.refused ? ["集計不可", "danger"] : attention(employee) ? ["要確認", "warning"] : ["集計済み", "success"];
  const reasons = {
    AmbiguousBoundary: "勤務の帰属が曖昧なため、集計を見合わせています。",
    PunchAnomaly: "打刻の順序や対に問題があります。元の打刻を確認してください。",
    HolidayBoundary: "勤務が法定休日の境界をまたいでいます。",
    WeekBoundary: "勤務が週の境界をまたいでいます。",
    MonthBoundary: "勤務が月の境界をまたいでいます。",
    IncompleteWeek: "週の初めから打刻がそろっているか確認してください。",
    OpenShift: "退勤の記録がありません。",
    OutWithoutIn: "対応する出勤がありません。出勤の記録と退勤の順序を確認してください。",
    DuplicateIn: "出勤が重複しています。出勤・退勤の順序を確認してください。",
    BreakStartWithoutIn: "勤務中でないときに休憩開始が記録されています。出勤と休憩の順序を確認してください。",
    BreakEndWithoutStart: "対応する休憩開始がありません。休憩開始と終了の記録を確認してください。",
    OutDuringBreak: "休憩を終了する前に退勤が記録されています。休憩終了の記録を確認してください。",
    SameTimestamp: "同じ時刻に複数の打刻があり、順序が決まりません。元の打刻の時刻を確認してください。",
    LongShift: "勤務が24時間以上に及んでいます。出勤・退勤の日付と打刻漏れを確認してください。",
  };
  const reason = (code) => reasons[code] || `記録の確認が必要です（${code}）。`;
  const issues = (employee) => {
    const messages = [];
    if (employee.refused) {
      messages.push(reason(employee.refused.code));
      if (employee.refused.dates.length) messages.push(`対象日: ${employee.refused.dates.join("、")}`);
      for (const anomaly of employee.refused.anomalies) {
        const location = anomaly.line == null ? anomaly.date : `${anomaly.date} · ${anomaly.line}行目`;
        messages.push(`${location}: ${reason(anomaly.kind)}`);
      }
    }
    for (const item of employee.break_shortages) messages.push(`${item.date}: 記録された休憩${item.break_minutes}分、必要な休憩${item.required_minutes}分（${item.shortage_minutes}分不足）。自動では差し引いていません。`);
    for (const item of employee.ambiguities) messages.push(`${item.date}: ${reason(item.code)} 現在の方針に基づく仮の集計です。`);
    return messages;
  };
  $("period").textContent = `${report.month.replace("-", "年")}月の集計 · ${report.period_start} 〜 ${report.period_end}`;
  document.title = `${report.month} 月次の確認 | kintai`;
  $("status-title").textContent = flagged.length ? `${flagged.length}人の記録を確認してください` : "集計を確認できます";
  $("status-description").textContent = refused.length
    ? `${refused.length}人は集計不可です。合計には含めていません。確認箇所を直し、レポートを再作成してください。`
    : flagged.length ? "休憩の不足や、仮の集計があります。各記録の理由を確認してください。" : "表示範囲に打刻の異常や休憩の不足はありません。法令への適合を保証するものではありません。";
  if (!refused.length && calculated.every((employee) => employee.ledger[0].work_days === 0)) {
    $("status-title").textContent = "対象期間の打刻がありません";
    $("status-description").textContent = "労働時間は0で表示しています。対象の月と打刻CSVを確認し、必要な記録をそろえて再作成してください。";
  }
  $("totals-scope").textContent = `集計できた ${calculated.length}人 / 全${employees.length}人 の合計`;
  const metric = (label, value, caption) => {
    const card = el("div", undefined, "metric");
    const number = el("p", undefined, "metric-value");
    const parts = value.match(/^(.+時間)(\d+分)$/);
    if (parts) number.append(el("span", parts[1]), document.createTextNode(" "), el("span", parts[2]));
    else number.textContent = value;
    card.append(el("p", label, "metric-label"), number, el("p", caption, "metric-caption"));
    $("metrics").append(card);
  };
  metric("労働時間", duration(sum("work_minutes")), "集計できた従業員のみ");
  metric("時間外", duration(sum("overtime_minutes")), "法定時間外の合計");
  metric("法定休日", duration(sum("holiday_minutes")), "事業場が定めた法定休日");
  metric("深夜", duration(sum("night_minutes")), "22時〜翌5時 · 他区分と重複");
  $("employee-count").textContent = `${employees.length}人`;
  $("check-count").textContent = `${flagged.length}人`;
  $("export-status").textContent = refused.length ? `一部集計できていません（${refused.length}人）。書き出すJSONにも、集計不可の理由を残します。` : "検索・表示の条件にかかわらず、このファイルに含まれる全従業員を書き出します。";
  $("metadata").textContent = `表の確認日: ${report.table_checked_on} · 状態: 専門家確認前（${report.review_status}） · ${report.schema}`;
  let filter = "all";
  let page = 0;
  const pageSize = 20;
  const labels = [["work_days", "労働日数"], ["within_minutes", "法定内"], ["extra_within_minutes", "所定外・法定内"], ["over60_minutes", "月60時間超"], ["holiday_minutes", "法定休日"], ["night_minutes", "深夜"]];
  function row(employee) {
    const detail = el("details", undefined, "employee-row");
    detail.dataset.employee = employee.employee_id;
    const summary = el("summary");
    const [label, color] = status(employee);
    summary.append(el("span", employee.employee_id, "employee-id"), el("span", label, `badge ${color}`));
    for (const [key, title] of [["work_minutes", "労働時間"], ["overtime_minutes", "時間外"], ["night_minutes", "深夜"]]) {
      const value = el("span", undefined, "row-number");
      value.append(el("span", title, "row-label"), document.createTextNode(employee.refused ? "—" : duration(employee.ledger[0]?.[key] ?? 0)));
      summary.append(value);
    }
    const chevron = el("span", "⌄", "chevron");
    chevron.setAttribute("aria-hidden", "true");
    summary.append(chevron);
    const body = el("div", undefined, "employee-detail");
    if (!employee.refused && employee.ledger.length) {
      const list = el("dl");
      for (const [key, title] of labels) {
        const group = el("div");
        group.append(el("dt", title), el("dd", key === "work_days" ? `${employee.ledger[0][key]}日` : duration(employee.ledger[0][key])));
        list.append(group);
      }
      body.append(list);
    }
    const messages = issues(employee);
    if (messages.length) {
      const list = el("ul");
      for (const message of messages) list.append(el("li", message));
      body.append(list);
    } else body.append(el("p", "この従業員の記録に、打刻の異常や休憩の不足はありません。"));
    detail.append(summary, body);
    return detail;
  }
  function render() {
    const query = $("search").value.trim().toLowerCase();
    const found = employees.filter((employee) => employee.employee_id.toLowerCase().includes(query)
      && (filter === "all" || (filter === "attention" ? attention(employee) : !employee.refused)));
    const pages = Math.max(1, Math.ceil(found.length / pageSize));
    page = Math.min(page, pages - 1);
    $("employee-list").replaceChildren(...found.slice(page * pageSize, (page + 1) * pageSize).map(row));
    $("result-count").textContent = `${found.length}人を表示対象にしています。概要の合計とJSONの対象範囲は変わりません。`;
    $("empty").hidden = found.length > 0;
    $("pagination").hidden = pages <= 1;
    $("page-label").textContent = `${page + 1} / ${pages}ページ`;
    $("previous").disabled = page === 0;
    $("next").disabled = page === pages - 1;
    document.querySelectorAll("[data-filter]").forEach((button) => button.setAttribute("aria-pressed", String(button.dataset.filter === filter)));
  }
  $("search").addEventListener("input", () => { page = 0; render(); });
  document.querySelectorAll("[data-filter]").forEach((button) => button.addEventListener("click", () => { filter = button.dataset.filter; page = 0; render(); }));
  $("reset").addEventListener("click", () => { $("search").value = ""; filter = "all"; page = 0; render(); $("search").focus(); });
  $("previous").addEventListener("click", () => { page--; render(); });
  $("next").addEventListener("click", () => { page++; render(); });
  render();
  function openEmployee(employee) {
    $("search").value = "";
    filter = "all";
    page = Math.floor(employees.indexOf(employee) / pageSize);
    render();
    const detail = Array.from($("employee-list").children).find((node) => node.dataset.employee === employee.employee_id);
    detail.open = true;
    const summary = detail.querySelector("summary");
    summary.focus();
    summary.scrollIntoView({ block:"center" });
  }
  for (const employee of flagged.slice(0, 20)) {
    const item = el("div", undefined, "check-item");
    const [label, color] = status(employee);
    const body = el("div");
    body.append(el("h3", employee.employee_id), el("p", issues(employee).join(" ")));
    const action = el("button", "記録を確認");
    action.type = "button";
    action.setAttribute("aria-label", `${employee.employee_id}の記録を確認`);
    action.addEventListener("click", () => openEmployee(employee));
    item.append(el("span", label, `badge ${color}`), body, action);
    $("check-list").append(item);
  }
  if (!flagged.length) $("check-list").append(el("p", "表示範囲に、確認が必要な記録はありません。", "check-clear"));
  if (flagged.length > 20) {
    const more = el("button", `ほか${flagged.length - 20}人を従業員の集計で確認`);
    more.type = "button";
    more.addEventListener("click", () => { $("search").value = ""; filter = "attention"; page = 0; render(); $("search").focus(); });
    $("check-list").append(more);
  }
  $("download").addEventListener("click", () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(report) + "\n"], { type:"application/json" }));
    const link = el("a");
    link.href = url;
    link.download = `kintai-${report.month}.json`;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  $("print").addEventListener("click", () => window.print());
})();
