const fs = require('fs');

// Piao v2 mobile UI
const piao = `{
  schema_version: 2,
  meta: {
    name: "Piao Mobile v2 - Manor Social Notebook",
    platform: "mobile",
  },
  layout: {
    root: {
      type: "stack",
      direction: "vertical",
      gap: "8px",
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
          class_name: "piao-m-header",
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
          class_name: "piao-m-focus",
        },
        {
          type: "component",
          component: "character_bar",
          class_name: "piao-m-chars",
        },
        {
          type: "component",
          component: "narration_card",
          class_name: "piao-m-narration",
          props: {
            title: "\u5bfc\u6f14\u63d0\u793a",
            show_copy_button: true,
          },
        },
        {
          type: "component",
          component: "message_list",
          class_name: "piao-m-messages",
          style: {
            flex: "1 1 0",
            min_height: "0",
          },
        },
        {
          type: "component",
          component: "side_panel_tabs",
          class_name: "piao-m-side",
          props: {
            show_map_tab: true,
            show_custom_tabs: true,
          },
        },
        { type: "component", component: "input_composer", class_name: "piao-m-input" },
        {
          type: "absolute",
          children: [
            {
              type: "component",
              component: "floating_actions",
              class_name: "piao-m-float",
              anchor: { top: "12px", right: "12px" },
              props: { show_back: true, show_debug: true, show_settings: true },
            },
          ],
        },
      ],
    },
  },
  tokens: {
    "color.bg": "#f8f6f3",
    "color.panel": "rgba(244, 237, 228, 0.92)",
    "color.text": "#2b2d3a",
    "color.text-dim": "#8e8174",
    "color.text-muted": "rgba(142, 129, 116, 0.65)",
    "color.border": "rgba(45, 71, 57, 0.12)",
    "color.input": "rgba(255, 255, 255, 0.60)",
    "color.accent": "#b88b8b",
    "color.accent-bg": "rgba(184, 139, 139, 0.12)",
    "color.deep": "#2d4739",
    "radius.md": "10px",
    "radius.lg": "12px",
    "font.body": "\\"Inter\\", \\"PingFang SC\\", \\"Microsoft YaHei\\", sans-serif",
    "font.display": "\\"Cormorant Garamond\\", \\"Source Serif 4\\", Georgia, serif",
    "motion.fast": "160ms",
  },
  components: {
    panel: {
      base: {
        background: "var(--game-ui-token-color-panel, rgba(244, 237, 228, 0.92))",
        border: "1px solid rgba(45, 71, 57, 0.10)",
        border_radius: "10px",
        box_shadow: "0 1px 3px rgba(43, 45, 58, 0.06)",
      },
    },
    button: {
      variants: {
        primary: { background: "#b88b8b", color: "#ffffff" },
        ghost: { background: "rgba(45, 71, 57, 0.06)", color: "#2b2d3a", border: "1px solid rgba(45, 71, 57, 0.10)" },
      },
    },
    message_bubble: {
      base: { border_radius: "10px" },
      variants: {
        agent: { background: "rgba(244, 237, 228, 0.95)", color: "#2b2d3a", border: "1px solid rgba(45, 71, 57, 0.08)" },
        player: { background: "rgba(184, 139, 139, 0.10)", color: "#2b2d3a", border: "1px solid rgba(184, 139, 139, 0.18)" },
        system: { background: "rgba(142, 129, 116, 0.06)", color: "#8e8174" },
      },
    },
  },
  effects: {
    page_enter: { enabled: true, duration: "180ms", easing: "ease-out" },
  },
  custom_css: \`
.game-ui-root.game-root {
  color: #2b2d3a;
  background: radial-gradient(circle at 20% 8%, rgba(255,239,220,0.28), transparent 22%), radial-gradient(circle at 80% 92%, rgba(184,139,139,0.10), transparent 18%), linear-gradient(180deg, #f8f6f3 0%, #f4ede4 45%, #ede3d6 100%);
  font-family: "Inter", "PingFang SC", "Microsoft YaHei", sans-serif;
}
.piao-m-header.game-simple-top { padding: 8px 14px; background: rgba(248,246,243,0.90); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); border-bottom: 1px solid rgba(45,71,57,0.08); border-radius: 0; }
.piao-m-header .game-simple-world { font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-size: 10px; font-weight: 700; letter-spacing: 0.10em; color: #8e8174; text-transform: uppercase; }
.piao-m-header .game-simple-place { font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-size: 17px; font-weight: 600; color: #2b2d3a; letter-spacing: 0.02em; }
.piao-m-header .game-simple-meta-item strong { color: #8e8174; }
.piao-m-header .game-simple-meta-item span { color: #2b2d3a; }
.piao-m-focus .game-scene-center { padding: 10px 14px; min-height: 140px; display: grid; grid-template-rows: 1fr auto; gap: 8px; align-items: end; justify-items: center; }
.piao-m-focus .game-avatar { max-width: min(55%, 160px); border: 1px solid rgba(45,71,57,0.08); border-radius: 6px; box-shadow: 0 2px 8px rgba(43,45,58,0.10); background: rgba(244,237,228,0.60); }
.piao-m-focus .game-current-line { font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-size: 15px; line-height: 1.80; padding: 10px 14px; max-width: 34ch; text-align: center; color: #2b2d3a; background: rgba(244,237,228,0.85); border: 1px solid rgba(45,71,57,0.08); border-radius: 8px; box-shadow: 0 1px 3px rgba(43,45,58,0.06); }
.piao-m-chars .game-scene-characters { padding: 4px 14px; gap: 6px; flex-wrap: wrap; }
.piao-m-chars .game-scene-char { font-size: 11px; padding: 2px 10px; border-radius: 4px; background: rgba(45,71,57,0.06); border: 1px solid rgba(45,71,57,0.10); color: #2d4739; font-weight: 600; }
.piao-m-narration .game-narration-panel { margin: 0 14px; padding: 8px 12px; background: rgba(184,139,139,0.08); border: 1px solid rgba(184,139,139,0.14); border-left: 3px solid #b88b8b; border-radius: 6px; }
.piao-m-narration .game-narration-label { font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-size: 11px; font-weight: 700; letter-spacing: 0.10em; color: #b88b8b; }
.piao-m-narration .game-narration-content { font-size: 13px; line-height: 1.70; color: #5a4e44; max-height: calc(1.70em * 3 + 4px); }
.piao-m-messages .game-chat-messages { padding: 8px 14px; gap: 10px; align-content: start; }
.piao-m-messages .game-message { border-radius: 10px; box-shadow: 0 1px 3px rgba(43,45,58,0.06); }
.piao-m-messages .game-message--agent { background: rgba(244,237,228,0.95); border: 1px solid rgba(45,71,57,0.08); border-bottom-left-radius: 4px; }
.piao-m-messages .game-message--player { background: rgba(184,139,139,0.08); border: 1px solid rgba(184,139,139,0.14); border-bottom-right-radius: 4px; }
.piao-m-messages .game-message--system { background: rgba(142,129,116,0.06); border: 1px solid rgba(142,129,116,0.08); border-radius: 6px; }
.piao-m-messages .game-message-speaker { font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-size: 12px; font-weight: 700; color: #b88b8b; letter-spacing: 0.04em; }
.piao-m-messages .game-message-speaker--player { color: #8e8174; }
.piao-m-messages .game-message-content { font-size: 14px; line-height: 1.65; color: #2b2d3a; }
.piao-m-messages .game-message-content--system { font-size: 12px; color: #8e8174; font-style: italic; }
.piao-m-side .game-status { margin: 0 14px; }
.piao-m-side .game-tabs { gap: 4px; }
.piao-m-side .game-tab { min-height: 28px; font-size: 11px; border-radius: 6px; background: rgba(45,71,57,0.04); border: 1px solid rgba(45,71,57,0.08); color: #8e8174; }
.piao-m-side .game-tab--active { background: #2d4739; color: #f8f6f3; border-color: #2d4739; }
.piao-m-side .game-panel { border-radius: 8px; background: rgba(244,237,228,0.70); border: 1px solid rgba(45,71,57,0.08); }
.piao-m-input .game-input-area { margin: 0; padding: 8px 14px calc(10px + env(safe-area-inset-bottom, 0px)); background: rgba(248,246,243,0.92); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); border-top: 1px solid rgba(45,71,57,0.08); }
.piao-m-input .game-textarea { background: rgba(255,255,255,0.60); border: 1px solid rgba(45,71,57,0.10); border-radius: 10px; font-size: 15px; color: #2b2d3a; min-height: 40px; max-height: 72px; }
.piao-m-input .game-textarea::placeholder { color: #b5a99a; font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; font-style: italic; }
.piao-m-input .game-submit-btn { border-radius: 8px; background: #b88b8b; color: #ffffff; border: none; min-height: 36px; font-size: 13px; }
.piao-m-input .game-input-icon-btn { color: #8e8174; border: none; }
.piao-m-input .game-session-meta-id { color: #b5a99a; font-size: 10px; }
.piao-m-float .game-mobile-shell-actions .game-back-btn,
.piao-m-float .game-mobile-shell-actions .game-quick-btn { background: rgba(248,246,243,0.88); border: 1px solid rgba(45,71,57,0.10); color: #2b2d3a; border-radius: 8px; backdrop-filter: blur(6px); -webkit-backdrop-filter: blur(6px); box-shadow: 0 1px 4px rgba(43,45,58,0.08); font-size: 12px; }
.piao-m-messages .game-message-action-btn { color: #b5a99a; border: none; border-radius: 6px; }
.piao-m-messages .game-message-action-btn:active { background: rgba(184,139,139,0.12); }
.piao-m-messages .game-message-inline-actions { border-top: 1px solid rgba(45,71,57,0.06); padding-top: 6px; }
.game-loading { color: #8e8174; font-family: "Cormorant Garamond","Source Serif 4",Georgia,serif; }
.game-error-text { color: #a0522d; }
\`,
}
`;

fs.writeFileSync('E:\\code\\rustweb\\examples\\world-packages\\piao-v2-ui\\mobile-ui.jsonc', piao, 'utf8');
console.log('Piao mobile written:', piao.length, 'bytes');
