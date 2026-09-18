world.register("ui.setTab", async (input) => {
  const allowed = ["chat", "growth", "profile"];
  const raw = input && input.tab != null ? String(input.tab) : "chat";
  return allowed.includes(raw) ? raw : "chat";
});
