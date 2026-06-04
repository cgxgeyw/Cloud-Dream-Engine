const fs = require('fs');

const feihualingMobile = `{
  schema_version: 2,
  meta: {
    name: "Feihualing Mobile - Moonlit Poetry Album",
    platform: "mobile",
  },
  layout: {
    root: {
      type: "stack",
      direction: "vertical",
      gap: "6px",
      padding: "0",
      style: {
        height: "100dvh",
        min_height: "0",
        overflow: "hidden",
      },
      children: [
        {
          type: "component",
          component: "scene_header",
          class_name: "fhl-m-header",
          props: {
            title_mode: "mobile",
            show_world_name: true,
            show_location: true,
            show_time_label: true,
            show_player_identity: false,
            show_visible_characters: false,
          },
        },
        {
          type: "component",
          component: "scene_focus",
          class_name: "fhl-m-focus",
        },
        {
          type: "component",
          component: "character_bar",
          class_name: "fhl-m-chars",
        },
        {
          type: "component",
          component: "narration_card",
          class_name: "fhl-m-narration",
          props: {
            title: "\u5e2d\u52bf\u63d0\u793a",
            show_copy_button: true,
          },
        },
        {
          type: "component",
          component: "message_list",
          class_name: "fhl-m-messages",
          style: {
            flex: "1 1 0",
            min_height: "0",
          },
        },
        {
          type: "component",
          component: "side_panel_tabs",
          class_name: "fhl-m-side",
          props: {
            show_map_tab: true,
            show_custom_tabs: true,
          },
        },
        { type: "component", component: "input_composer", class_name: "fhl-m-input" },
        {
          type: "absolute",
          children: [
            {
              type: "component",
              component: "floating_actions",
              class_name: "fhl-m-float",
              anchor: { top: "12px", right: "12px" },
              props: { show_back: true, show_debug: true, show_settings: true },
            },
          ],
        },
      ],
    },
  },
  tokens: {
    "color.bg": "#eee8e0",
    "color.panel": "rgba(232, 226, 216, 0.92)",
    "color.text": "#1a1a2e",
    "color.text-dim": "#6e6e80",
    "color.text-muted": "rgba(100, 100, 120, 0.60)",
    "color.border": "rgba(26, 26, 46, 0.10)",
    "color.input": "rgba(255, 255, 255, 0.50)",
    "color.accent": "#5a7a8a",
    "color.accent-bg": "rgba(90, 122, 138, 0.10)",
    "color.deep": "#2a3a4a",
    "radius.md": "8px",
    "radius.lg": "10px",
    "font.body": "\\"PingFang SC\\", \\"Source Han Sans SC\\", \\"Microsoft YaHei\\", sans-serif",
    "font.display": "\\"Source Han Serif SC\\", \\"Noto Serif SC\\", \\"SimSun\\", serif",
    "motion.fast": "180ms",
  },
  components: {
    panel: {
      base: {
        background: "var(--game-ui-token-color-panel, rgba(232, 226, 216, 0.92))",
        border: "1px solid rgba(26, 26, 46, 0.08)",
        border_radius: "8px",
        box_shadow: "0 1px 2px rgba(26, 26, 46, 0.05)",
      },
    },
    button: {
      variants: {
        primary: { background: "#5a7a8a", color: "#ffffff" },
        ghost: { background: "rgba(26, 26, 46, 0.04)", color: "#1a1a2e", border: "1px solid rgba(26, 26, 46, 0.08)" },
      },
    },
    message_bubble: {
      base: { border_radius: "8px" },
      variants: {
        agent: { background: "rgba(232, 226, 216, 0.95)", color: "#1a1a2e", border: "1px solid rgba(26, 26, 46, 0.06)" },
        player: { background: "rgba(90, 122, 138, 0.08)", color: "#1a1a2e", border: "1px solid rgba(90, 122, 138, 0.14)" },
        system: { background: "rgba(100, 100, 120, 0.05)", color: "#6e6e80" },
      },
    },
  },
  effects: {
    page_enter: { enabled: true, duration: "200ms", easing: "ease-out" },
  },
  custom_css: \`
.game-ui-root.game-root {
  color: #1a1a2e;
  background: radial-gradient(circle at 50% 0%, rgba(200,210,220,0.30), transparent 30%), linear-gradient(180deg, #eee8e0 0%, #e4ddd4 50%, #dcd5ca 100%);
  font-family: "PingFang SC", "Source Han Sans SC", "Microsoft YaHei", sans-serif;
}
.fhl-m-header.game-simple-top { padding: 8px 14px; background: rgba(238,232,224,0.92); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); border-bottom: 1px solid rgba(26,26,46,0.06); border-radius: 0; }
.fhl-m-header .game-simple-world { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-size: 10px; font-weight: 700; letter-spacing: 0.12em; color: #6e6e80; text-transform: uppercase; }
.fhl-m-header .game-simple-place { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-size: 17px; font-weight: 600; color: #1a1a2e; letter-spacing: 0.04em; }
.fhl-m-header .game-simple-meta-item strong { color: #6e6e80; }
.fhl-m-header .game-simple-meta-item span { color: #1a1a2e; }
.fhl-m-focus .game-scene-center { padding: 10px 14px; min-height: 130px; display: grid; grid-template-rows: 1fr auto; gap: 8px; align-items: end; justify-items: center; }
.fhl-m-focus .game-avatar { max-width: min(50%, 140px); border: 1px solid rgba(26,26,46,0.06); border-radius: 4px; box-shadow: 0 1px 4px rgba(26,26,46,0.08); background: rgba(232,226,216,0.50); }
.fhl-m-focus .game-current-line { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-size: 15px; line-height: 1.85; padding: 10px 14px; max-width: 32ch; text-align: center; color: #1a1a2e; background: rgba(232,226,216,0.80); border: 1px solid rgba(26,26,46,0.06); border-radius: 6px; letter-spacing: 0.02em; }
.fhl-m-chars .game-scene-characters { padding: 4px 14px; gap: 6px; flex-wrap: wrap; }
.fhl-m-chars .game-scene-char { font-size: 11px; padding: 2px 10px; border-radius: 3px; background: rgba(26,26,46,0.04); border: 1px solid rgba(26,26,46,0.08); color: #2a3a4a; font-weight: 600; }
.fhl-m-narration .game-narration-panel { margin: 0 14px; padding: 8px 12px; background: rgba(90,122,138,0.06); border: 1px solid rgba(90,122,138,0.10); border-left: 3px solid #5a7a8a; border-radius: 4px; }
.fhl-m-narration .game-narration-label { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-size: 11px; font-weight: 700; letter-spacing: 0.12em; color: #5a7a8a; }
.fhl-m-narration .game-narration-content { font-size: 13px; line-height: 1.70; color: #3a3a4e; max-height: calc(1.70em * 3 + 4px); }
.fhl-m-messages .game-chat-messages { padding: 8px 14px; gap: 10px; align-content: start; }
.fhl-m-messages .game-message { border-radius: 8px; box-shadow: 0 1px 2px rgba(26,26,46,0.05); }
.fhl-m-messages .game-message--agent { background: rgba(232,226,216,0.95); border: 1px solid rgba(26,26,46,0.06); border-bottom-left-radius: 3px; }
.fhl-m-messages .game-message--player { background: rgba(90,122,138,0.06); border: 1px solid rgba(90,122,138,0.10); border-bottom-right-radius: 3px; }
.fhl-m-messages .game-message--system { background: rgba(100,100,120,0.04); border: 1px solid rgba(100,100,120,0.06); border-radius: 4px; }
.fhl-m-messages .game-message-speaker { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-size: 12px; font-weight: 700; color: #5a7a8a; letter-spacing: 0.06em; }
.fhl-m-messages .game-message-speaker--player { color: #6e6e80; }
.fhl-m-messages .game-message-content { font-size: 14px; line-height: 1.70; color: #1a1a2e; }
.fhl-m-messages .game-message-content--system { font-size: 12px; color: #6e6e80; font-style: italic; }
.fhl-m-side .game-status { margin: 0 14px; }
.fhl-m-side .game-tabs { gap: 4px; }
.fhl-m-side .game-tab { min-height: 28px; font-size: 11px; border-radius: 4px; background: rgba(26,26,46,0.03); border: 1px solid rgba(26,26,46,0.06); color: #6e6e80; }
.fhl-m-side .game-tab--active { background: #2a3a4a; color: #eee8e0; border-color: #2a3a4a; }
.fhl-m-side .game-panel { border-radius: 6px; background: rgba(232,226,216,0.60); border: 1px solid rgba(26,26,46,0.06); }
.fhl-m-input .game-input-area { margin: 0; padding: 8px 14px calc(10px + env(safe-area-inset-bottom, 0px)); background: rgba(238,232,224,0.92); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); border-top: 1px solid rgba(26,26,46,0.06); }
.fhl-m-input .game-textarea { background: rgba(255,255,255,0.50); border: 1px solid rgba(26,26,46,0.08); border-radius: 8px; font-size: 15px; color: #1a1a2e; min-height: 40px; max-height: 72px; }
.fhl-m-input .game-textarea::placeholder { color: #a0a0b0; font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; font-style: italic; }
.fhl-m-input .game-submit-btn { border-radius: 6px; background: #5a7a8a; color: #ffffff; border: none; min-height: 36px; font-size: 13px; }
.fhl-m-input .game-input-icon-btn { color: #6e6e80; border: none; }
.fhl-m-input .game-session-meta-id { color: #a0a0b0; font-size: 10px; }
.fhl-m-float .game-mobile-shell-actions .game-back-btn,
.fhl-m-float .game-mobile-shell-actions .game-quick-btn { background: rgba(238,232,224,0.88); border: 1px solid rgba(26,26,46,0.08); color: #1a1a2e; border-radius: 6px; backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); box-shadow: 0 1px 4px rgba(26,26,46,0.06); font-size: 12px; }
.fhl-m-messages .game-message-action-btn { color: #a0a0b0; border: none; border-radius: 4px; }
.fhl-m-messages .game-message-action-btn:active { background: rgba(90,122,138,0.10); }
.fhl-m-messages .game-message-inline-actions { border-top: 1px solid rgba(26,26,46,0.05); padding-top: 6px; }
.game-loading { color: #6e6e80; font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; }
.game-error-text { color: #8b4513; }
\`,
}
`;

const feihualingDesktop = `{
  schema_version: 2,
  meta: {
    name: "Feihualing Desktop - Moonlit Poetry Album",
    platform: "desktop",
  },
  layout: {
    root: {
      type: "grid",
      columns: ["minmax(280px, 1fr)", "minmax(420px, 1.35fr)", "300px"],
      rows: ["auto", "minmax(0, 1fr)"],
      areas: [
        ["header", "header", "actions"],
        ["scene", "chat", "side"],
      ],
      gap: "14px",
      padding: "16px",
      style: {
        height: "100%",
        min_height: "0",
      },
      children: [
        {
          type: "component",
          component: "scene_header",
          area: "header",
          props: {
            show_world_name: true,
            show_location: true,
            show_time_label: true,
            show_player_identity: true,
            player_identity_format: "action_phrase",
            show_visible_characters: false,
            title_mode: "desktop",
          },
        },
        {
          type: "component",
          component: "floating_actions",
          area: "actions",
          props: { show_back: true, show_debug: true, show_settings: true },
        },
        {
          type: "stack",
          area: "scene",
          gap: "10px",
          children: [
            { type: "component", component: "scene_focus" },
            { type: "component", component: "character_bar" },
            {
              type: "component",
              component: "narration_card",
              props: { title: "\u5e2d\u52bf\u63d0\u793a", show_copy_button: true },
            },
          ],
        },
        {
          type: "stack",
          area: "chat",
          gap: "10px",
          style: { min_height: "0", height: "100%" },
          children: [
            {
              type: "component",
              component: "message_list",
              style: { flex: "1 1 0", min_height: "0" },
            },
            { type: "component", component: "input_composer" },
          ],
        },
        {
          type: "component",
          component: "side_panel_tabs",
          area: "side",
          props: { show_map_tab: true, show_custom_tabs: true },
        },
      ],
    },
  },
  tokens: {
    "color.bg": "#eee8e0",
    "color.panel": "rgba(232, 226, 216, 0.92)",
    "color.text": "#1a1a2e",
    "color.text-dim": "#6e6e80",
    "color.border": "rgba(26, 26, 46, 0.10)",
    "color.input": "rgba(255, 255, 255, 0.50)",
    "color.accent": "#5a7a8a",
    "color.deep": "#2a3a4a",
    "radius.lg": "20px",
    "font.body": "\\"PingFang SC\\", \\"Source Han Sans SC\\", \\"Microsoft YaHei\\", sans-serif",
    "font.display": "\\"Source Han Serif SC\\", \\"Noto Serif SC\\", \\"SimSun\\", serif",
    "motion.fast": "180ms",
  },
  components: {
    panel: {
      base: {
        background: "var(--game-ui-token-color-panel, rgba(232, 226, 216, 0.92))",
        border: "1px solid rgba(26, 26, 46, 0.08)",
        border_radius: "20px",
        box_shadow: "0 4px 16px rgba(26, 26, 46, 0.06)",
      },
    },
    button: {
      variants: {
        primary: { background: "#5a7a8a", color: "#ffffff" },
        ghost: { background: "rgba(26, 26, 46, 0.04)", color: "#1a1a2e", border: "1px solid rgba(26, 26, 46, 0.08)" },
      },
    },
    message_bubble: {
      base: { border_radius: "14px" },
      variants: {
        agent: { background: "rgba(232, 226, 216, 0.95)", color: "#1a1a2e", border: "1px solid rgba(26, 26, 46, 0.06)" },
        player: { background: "rgba(90, 122, 138, 0.08)", color: "#1a1a2e", border: "1px solid rgba(90, 122, 138, 0.14)" },
        system: { background: "rgba(100, 100, 120, 0.05)", color: "#6e6e80" },
      },
    },
  },
  effects: {
    page_enter: { enabled: true, duration: "200ms", easing: "ease-out" },
  },
  custom_css: \`
.game-ui-root.game-root {
  color: #1a1a2e;
  background: radial-gradient(circle at 50% 0%, rgba(200,210,220,0.20), transparent 30%), linear-gradient(180deg, #eee8e0 0%, #e4ddd4 50%, #dcd5ca 100%);
  font-family: "PingFang SC", "Source Han Sans SC", "Microsoft YaHei", sans-serif;
}
.game-header { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; }
.game-scene-name { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; letter-spacing: 0.06em; }
.game-scene-center { border-radius: 12px; }
.game-avatar { border-radius: 6px; box-shadow: 0 2px 8px rgba(26,26,46,0.08); }
.game-current-line { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; line-height: 1.85; letter-spacing: 0.02em; }
.game-message { border-radius: 14px; box-shadow: 0 2px 6px rgba(26,26,46,0.05); }
.game-message--agent { background: rgba(232,226,216,0.95); border: 1px solid rgba(26,26,46,0.06); border-bottom-left-radius: 6px; }
.game-message--player { background: rgba(90,122,138,0.06); border: 1px solid rgba(90,122,138,0.10); border-bottom-right-radius: 6px; }
.game-message-speaker { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; color: #5a7a8a; letter-spacing: 0.06em; }
.game-textarea { border-radius: 14px; background: rgba(255,255,255,0.50); border: 1px solid rgba(26,26,46,0.08); }
.game-textarea::placeholder { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; color: #a0a0b0; font-style: italic; }
.game-submit-btn { background: #5a7a8a; color: #fff; border: none; border-radius: 10px; }
.game-tab { border-radius: 6px; background: rgba(26,26,46,0.03); border: 1px solid rgba(26,26,46,0.06); color: #6e6e80; }
.game-tab--active { background: #2a3a4a; color: #eee8e0; border-color: #2a3a4a; }
.game-narration-label { font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; color: #5a7a8a; letter-spacing: 0.12em; }
.game-scene-char { border-radius: 3px; background: rgba(26,26,46,0.04); border: 1px solid rgba(26,26,46,0.08); color: #2a3a4a; }
.game-loading { color: #6e6e80; font-family: "Source Han Serif SC","Noto Serif SC","SimSun",serif; }
.game-error-text { color: #8b4513; }
\`,
}
`;

const readme = `# Feihualing v2 UI

Feihualing (Fly Flower Token / Poem Round) world package with moonlit poetry album aesthetic.

## Aesthetic

- **Palette**: Moon white, ink blue, stone blue, cold jade, light gold
- **Typography**: Source Han Serif SC for headings, PingFang SC for body
- **Material**: Xuan paper, ink traces, lacquer wood, fine metal edges
- **Mood**: Cool, restrained, literary sparring

## Files

- \`mobile-ui.jsonc\` - Mobile layout (schema v2)
- \`desktop-ui.jsonc\` - Desktop layout (schema v2)

## Related

- See \`docs/piao-feihualing-ui-redesign-2026-06-01.md\` for design rationale
- See \`data/Desktop Narrative Game Interface/guidelines/Guidelines-Piao.md\` for 飘 guidelines
`;

const dir = 'E:\\code\\rustweb\\examples\\world-packages\\feihualing-v2-ui';
fs.writeFileSync(dir + '\\mobile-ui.jsonc', feihualingMobile, 'utf8');
fs.writeFileSync(dir + '\\desktop-ui.jsonc', feihualingDesktop, 'utf8');
fs.writeFileSync(dir + '\\README.md', readme, 'utf8');
console.log('Feihualing files written:');
console.log('  mobile-ui.jsonc:', feihualingMobile.length, 'bytes');
console.log('  desktop-ui.jsonc:', feihualingDesktop.length, 'bytes');
console.log('  README.md:', readme.length, 'bytes');
