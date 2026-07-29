world.register("starter.sessionStart", async (_input, api) => {
  const route = await api.kv.get("variables", "route", "", { scope: "session" });
  if (!route) {
    await api.kv.set("variables", "route", "未选择", { scope: "session" });
  }
  return { route: route || "未选择" };
});

world.register("starter.interactionAnswered", async (input, api) => {
  const routes = {
    join: "投奔义军",
    defend: "守护乡里",
    flee: "避乱南下"
  };
  const route = routes[String(input.answer || "")];
  if (!route) {
    return { ignored: true };
  }
  await api.kv.set("variables", "route", route, { scope: "session" });
  return { route };
});
