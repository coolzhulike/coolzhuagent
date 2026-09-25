(() => {
  const filterButtons = [...document.querySelectorAll("[data-filter]")];
  const rows = [...document.querySelectorAll("#test-rows tr")];
  const summary = document.querySelector("#matrix-summary");
  const labels = { all: "全部", pass: "通过", blocked: "受限", pending: "待测" };
  const counts = rows.reduce((result, row) => {
    const status = row.dataset.status || "pending";
    result.all += 1;
    result[status] = (result[status] || 0) + 1;
    return result;
  }, { all: 0, pass: 0, blocked: 0, pending: 0 });

  document.querySelectorAll("[data-filter-count]").forEach((node) => {
    node.textContent = `(${counts[node.dataset.filterCount] || 0})`;
  });

  const applyFilter = (filter) => {
    let visibleCount = 0;
    filterButtons.forEach((item) => {
      const active = item.dataset.filter === filter;
      item.classList.toggle("active", active);
      item.setAttribute("aria-pressed", String(active));
    });
    rows.forEach((row) => {
      const hidden = filter !== "all" && row.dataset.status !== filter;
      row.classList.toggle("hidden", hidden);
      if (!hidden) visibleCount += 1;
    });
    if (summary) {
      summary.textContent = `当前显示：${labels[filter] || labels.all} ${visibleCount} 项；共 ${counts.all} 项。`;
    }
  };

  for (const button of filterButtons) {
    button.addEventListener("click", () => {
      const filter = button.dataset.filter || "all";
      applyFilter(filter);
    });
  }
  applyFilter("all");

  const links = [...document.querySelectorAll(".dock a")];
  const sections = links
    .map((link) => document.querySelector(link.getAttribute("href")))
    .filter(Boolean);
  if ("IntersectionObserver" in window) {
    const observer = new IntersectionObserver((entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((left, right) => right.intersectionRatio - left.intersectionRatio)[0];
      if (!visible) return;
      links.forEach((link) => link.classList.toggle(
        "active",
        link.getAttribute("href") === `#${visible.target.id}`,
      ));
    }, { rootMargin: "-25% 0px -60%", threshold: [0.05, 0.25, 0.5] });
    sections.forEach((section) => observer.observe(section));
  }
})();
