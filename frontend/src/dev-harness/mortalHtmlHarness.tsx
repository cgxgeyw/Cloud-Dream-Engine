import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { WorldFrameHost } from '../components/GameUiSandboxPreview';
import { parseGameUiDocument } from '../data/gameUi/parser';
import type { WorldFrameRuntimePayload } from '../worldFrame/runtimeSnapshot';
import type { WorldFrameAction } from '../worldFrame/protocol';
import desktop from '../../../examples/world-packages/mortal-cultivation-journey/world/ui.desktop.html?raw';
import mobile from '../../../examples/world-packages/mortal-cultivation-journey/world/ui.mobile.html?raw';
import worldData from '../../../examples/world-packages/mortal-cultivation-journey/world/world.json';

const platform = new URLSearchParams(location.search).get('platform') === 'mobile' ? 'mobile' : 'desktop';
const items = worldData.attribute_schemas.map(s => ({ ...s, schema_id: s.key, value: s.default_value }));
const groups = [...new Set(items.map(s => s.display_policy.group))];
const baseSnapshot = {
  world: { name: worldData.name, id: 'test-world' },
  player: { name: '陆谨', id: 'test-player' },
  session: { id: 'test-session', location: '七玄门山门', time_label: '清晨', player_character_name: '陆谨' },
  map_graph: {
    nodes: ['村落', '山门', '炼骨崖', '神手谷', '百锻堂', '弟子院'].map((label, i) => ({ node_id: String(i), label, discovered: i < 4, current: i === 1 })),
    // A fork, a merge, and a cycle. Display must follow edges rather than array order.
    edges: [[0, 1], [1, 2], [1, 3], [2, 4], [3, 4], [4, 1]].map(([s, t], i) => ({ edge_id: String(i), source_node_id: String(s), target_node_id: String(t) })),
  },
  attributes: Object.fromEntries(items.map(s => [s.key, s.value])),
  attribute_items: items,
  visible_characters: ['韩立', '张铁', '王护法'],
  side_tabs: [{ key: 'map', label: '地图' }, ...groups.map(label => ({ key: `attribute:${label}`, label })), ...(groups.includes('背包') ? [] : [{ key: 'attribute:背包', label: '背包' }])],
  active_side_tab: 'map', active_attribute_items: [], active_attribute_content: '',
  messages: [
    { message_id: 'director', role: 'system', content: '安排入门试炼', metadata: { action_type: 'director_trace', reasoning: '根据当前场景安排角色。'.repeat(25), trace_text: '安排入门试炼' } },
    { message_id: 'player', role: 'user', content: '观察山门。', metadata: { turn_index: 1 } },
    { message_id: 'interaction', role: 'system', content: '选择路线', metadata: { action_type: 'world_interaction', message_kind: 'world_interaction', interaction: { interaction_id: 'interaction-1', kind: 'choice', prompt: '你准备走哪条路线？', config: { options: [{ id: 'ridge', label: '沿山脊前进' }, { id: 'stream', label: '顺溪谷绕行' }] }, status: 'pending' } } },
    { message_id: 'confirm-interaction', role: 'system', content: '确认入门试炼', metadata: { action_type: 'world_interaction', message_kind: 'world_interaction', interaction: { interaction_id: 'interaction-2', kind: 'confirm', prompt: '是否开始入门试炼？', config: { confirm_label: '开始试炼', cancel_label: '暂不开始' }, status: 'pending' } } },
    { message_id: 'slider-interaction', role: 'system', content: '选择行进速度', metadata: { action_type: 'world_interaction', message_kind: 'world_interaction', interaction: { interaction_id: 'interaction-3', kind: 'slider', prompt: '选择行进速度', config: { min: 1, max: 10, step: 1, default: 4 }, status: 'pending' } } },
    { message_id: 'npc', role: 'agent', speaker: '王护法', content: '随我来，莫要掉队。', metadata: { reasoning: '仔细观察这些新来的少年。'.repeat(25), narration: '晨雾渐散。', turn_index: 1, streaming: true } },
  ],
  ui_state: { submitting: false }, errors: { action_error: null },
  draft_input: { value: '', images: [], audios: [], is_recording: false },
  viewport: { height: innerHeight, keyboard_height: 0, safe_area: { top: 0, right: 0, bottom: 0, left: 0 } },
  message_preferences: { auto_scroll_enabled: true },
};

function Harness() {
  const [snapshot, setSnapshot] = useState(baseSnapshot);
  const testWindow = window as unknown as { received: WorldFrameAction[]; failSubmit: boolean; pushSnapshot: (patch: object) => void };
  testWindow.received ??= [];
  testWindow.pushSnapshot = patch => setSnapshot(s => ({ ...s, ...patch }));
  const onAction = async (action: WorldFrameAction) => {
    testWindow.received.push(action);
    if (action.type === 'clear-action-error') setSnapshot(s => ({ ...s, errors: { action_error: null } }));
    if (action.type === 'switch-side-tab') {
      const contentOnly = action.tabKey === 'attribute:背包';
      setSnapshot(s => ({ ...s, active_side_tab: action.tabKey,
        active_attribute_items: contentOnly ? [] : items.filter(i => `attribute:${i.display_policy.group}` === action.tabKey) as never[],
        active_attribute_content: contentOnly ? '干粮 x3\n旧水囊 x1\n铜钱 x18' : '',
      }));
    }
    if (action.type === 'set-draft-value') setSnapshot(s => ({ ...s, draft_input: { ...s.draft_input, value: action.value } }));
    if (action.type === 'submit-message') {
      if (testWindow.failSubmit) throw new Error('测试：模型服务不可用，请重试');
      setSnapshot(s => ({ ...s, messages: [...s.messages, { message_id: 'sent', role: 'user', content: action.options.content || '', metadata: { turn_index: 2 } } as never], draft_input: { ...s.draft_input, value: '' } }));
      return { accepted: true };
    }
    if (action.type === 'answer-interaction') {
      setSnapshot(s => ({ ...s, messages: s.messages.map(message => {
        if (message.message_id !== action.messageId) return message;
        const metadata = { ...(message.metadata ?? {}) } as Record<string, unknown>;
        const interaction = metadata.interaction as Record<string, unknown>;
        metadata.interaction = { ...interaction, status: 'answered', answer: action.answer };
        return { ...message, metadata } as never;
      }) }));
    }
  };
  const payload = { platform, document: parseGameUiDocument(platform === 'mobile' ? mobile : desktop, platform).document, stylesheet: '', snapshot } as unknown as WorldFrameRuntimePayload;
  return <WorldFrameHost mode="runtime" payload={payload} title="Mortal HTML runtime" onAction={onAction} />;
}

const style = document.createElement('style');
style.textContent = 'html,body,#root,.world-ui-sandbox-preview{height:100%;margin:0}.world-ui-sandbox-preview{position:relative}.world-ui-sandbox-preview-frame{width:100%;height:100%;border:0}.world-ui-sandbox-preview-status{position:absolute;inset:0;display:grid;place-items:center;pointer-events:none;background:#e8e1d3}.world-ui-sandbox-preview-status--error{color:#963c31}';
document.head.append(style);
createRoot(document.getElementById('root')!).render(<Harness />);
