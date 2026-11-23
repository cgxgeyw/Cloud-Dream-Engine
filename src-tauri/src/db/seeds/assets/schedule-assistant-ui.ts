// Authoring source. scripts/build-assistant-world-ui.mjs embeds the compiled JS in both HTML entries.
type Snapshot = {
  world?: { name?: string };
  session?: { time_label?: string | null; location?: string | null };
  player?: { name?: string };
  attributes?: Record<string, unknown>;
  messages?: Message[];
  draft_input?: { value?: string; images?: Attachment[]; audios?: Attachment[]; is_recording?: boolean; microphone_error?: string | null };
  editing?: { turnIndex: number; originalContent: string } | null;
  ui_state?: { submitting?: boolean; page_error?: string | null };
  errors?: { action_error?: string | null; page_error?: string | null };
};
type ContentPart = { type: string; text?: string; image_url?: { url?: string }; input_audio?: { data?: string; format?: string } };
type Message = { message_id?: string; role: string; content: string | ContentPart[]; speaker?: string | null; metadata?: Record<string, unknown> | null; pending?: boolean };
type Attachment = { id?: string; name?: string; size?: number; type?: string; preview_url?: string };
type Actions = {
  submitMessage(options: { content?: string; mode?: 'submit' | 'edit' | 'resend'; turnIndex?: number }): Promise<unknown>;
  clearError(): Promise<unknown>;
  copyText(text: string): Promise<unknown>;
  branch(): Promise<unknown>;
  editTurn(content: string, turnIndex: number): Promise<unknown>;
  cancelEdit(): Promise<unknown>;
  startRecording(): Promise<unknown>;
  stopRecording(send?: boolean): Promise<unknown>;
  removeAudio(index: number): Promise<unknown>;
  answerInteraction(messageId: string, interactionId: string, answer: unknown): Promise<unknown>;
};
type WorldSdk = { getState(): Snapshot; subscribe(listener: (state: Snapshot) => void): () => void; actions: Actions };

const world = (window as unknown as { world: WorldSdk }).world;
const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T | null;
const qsa = <T extends HTMLElement = HTMLElement>(selector: string) => Array.from(document.querySelectorAll(selector)) as T[];
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text = '') => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  return node;
};
const valueText = (value: unknown): string => typeof value === 'string' ? value : value == null ? '' : JSON.stringify(value);
const input = $<HTMLTextAreaElement>('input')!;
const messages = $('messages')!;
const statusNode = $('status')!;
const send = $<HTMLButtonElement>('send')!;
let state: Snapshot = {};
let tab = 'chat';
let sending = false;
let localError = '';
let draftDirty = false;
let forceFollow = false;
let messagesKey = '';
let editingTurn: number | undefined;
const expanded = new Set<string>();
const answering = new Set<string>();
const checkedItems = new Set<string>();

function errorText(error: unknown) { return error instanceof Error ? error.message : String(error); }
function showError(error: unknown) {
  localError = errorText(error);
  const banner = $('page-error');
  if (banner) { banner.textContent = localError; banner.hidden = false; }
  updateStatus();
}
function run(work: () => Promise<unknown>) { void work().catch(showError); }
function setText(id: string, text: string) { const node = $(id); if (node) node.textContent = text; }
function clearError() {
  localError = '';
  const banner = $('page-error');
  if (banner) banner.hidden = true;
  updateStatus();
}
function attribute(key: string) { return state.attributes?.[key]; }
function listAttribute(key: string): string[] {
  const value = attribute(key);
  if (Array.isArray(value)) return value.map((item) => valueText(item)).filter(Boolean);
  if (typeof value === 'string' && value) {
    try {
      const parsed = JSON.parse(value);
      if (Array.isArray(parsed)) return parsed.map((item) => valueText(item)).filter(Boolean);
    } catch {
      return value.split('\n').map((item) => item.trim()).filter(Boolean);
    }
  }
  return [];
}
function messageText(message: Message) {
  if (typeof message.content === 'string') return message.content;
  return message.content.filter((part) => part.type === 'text').map((part) => part.text || '').join('\n');
}
function messageParts(message: Message) { return typeof message.content === 'string' ? [] : message.content; }

function updateStatus() {
  const error = localError || state.errors?.action_error || state.errors?.page_error || state.ui_state?.page_error;
  statusNode.textContent = error || (sending || state.ui_state?.submitting ? '行程助手正在安排…' : '');
  statusNode.setAttribute('role', error ? 'alert' : 'status');
  statusNode.classList.toggle('has-error', Boolean(error));
  send.disabled = sending || !!state.ui_state?.submitting;
  send.textContent = sending || state.ui_state?.submitting ? '发送中' : state.editing ? '保存并重发' : '发送';
}
async function submit(options?: { content?: string; mode?: 'submit' | 'edit' | 'resend'; turnIndex?: number }): Promise<boolean> {
  if (sending || state.ui_state?.submitting) { showError('上一条消息仍在处理，请稍候。'); return false; }
  const content = options?.content ?? input.value.trim();
  if (!content && !state.draft_input?.audios?.length) { showError('先写下要安排的事情，或录一段语音。'); return false; }
  sending = true;
  localError = '';
  forceFollow = true;
  updateStatus();
  try {
    await world.actions.clearError();
    await world.actions.submitMessage({ content, mode: options?.mode ?? (state.editing ? 'edit' : 'submit'), turnIndex: options?.turnIndex ?? state.editing?.turnIndex });
    if (!options && input.value.trim() === content) { input.value = ''; draftDirty = false; }
    return true;
  } catch (error) { showError(error); return false; }
  finally { sending = false; updateStatus(); }
}
send.type = 'button';
send.onclick = () => void submit();
input.oninput = () => { draftDirty = true; };
input.onkeydown = (event) => {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing && document.body.dataset.platform !== 'mobile') {
    event.preventDefault();
    void submit();
  }
};
window.addEventListener('world-error', (event) => showError((event as CustomEvent<{ message: string }>).detail.message));
$('voice')?.addEventListener('click', () => run(() => state.draft_input?.is_recording ? world.actions.stopRecording() : world.actions.startRecording()));
$('cancel-edit')?.addEventListener('click', () => run(() => world.actions.cancelEdit()));

function cot(text: string, title: string, key: string, streaming: boolean) {
  const root = el('section', 'game-cot');
  root.dataset.streaming = String(streaming);
  root.append(el('div', 'game-cot-label', title), el('div', 'game-cot-content', expanded.has(key) ? text : text.slice(0, 200)));
  if (text.length > 200) {
    const toggle = el('button', 'cot-toggle', expanded.has(key) ? '收起' : '展开全部');
    toggle.type = 'button';
    toggle.onclick = () => { if (expanded.has(key)) expanded.delete(key); else expanded.add(key); messagesKey = ''; renderMessages(state); };
    root.append(toggle);
  }
  return root;
}
type Interaction = { interaction_id: string; kind: string; prompt: string; config: Record<string, unknown>; status: string; answer?: unknown };
function interactionFrom(value: unknown): Interaction | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const item = value as Record<string, unknown>;
  if (typeof item.interaction_id !== 'string' || typeof item.kind !== 'string') return null;
  return {
    interaction_id: item.interaction_id,
    kind: item.kind,
    prompt: typeof item.prompt === 'string' ? item.prompt : '',
    config: item.config && typeof item.config === 'object' && !Array.isArray(item.config) ? item.config as Record<string, unknown> : {},
    status: typeof item.status === 'string' ? item.status : 'pending',
    answer: item.answer,
  };
}
function renderInteraction(messageId: string, interaction: Interaction, submitting: boolean) {
  const root = el('article', 'message message--interaction');
  root.append(el('div', 'speaker', '需要你决定'), el('div', 'interaction-prompt', interaction.prompt));
  const answerKey = `${messageId}:${interaction.interaction_id}`;
  const disabled = interaction.status !== 'pending' || submitting || answering.has(answerKey) || !messageId;
  const options = Array.isArray(interaction.config.options) ? interaction.config.options.filter((item): item is Record<string, unknown> => !!item && typeof item === 'object' && typeof (item as Record<string, unknown>).id === 'string') : [];
  const answer = (value: unknown) => {
    if (disabled) return;
    answering.add(answerKey);
    messagesKey = '';
    renderMessages(state);
    void world.actions.answerInteraction(messageId, interaction.interaction_id, value)
      .catch(showError)
      .finally(() => { answering.delete(answerKey); messagesKey = ''; renderMessages(state); });
  };
  if (interaction.status !== 'pending') {
    root.append(el('div', 'interaction-answer', '已提交'));
    return root;
  }
  const controls = el('div', 'interaction-controls');
  if (interaction.kind === 'choice') {
    for (const option of options) {
      const button = el('button', 'interaction-option', valueText(option.label || option.id));
      button.type = 'button';
      button.disabled = disabled;
      button.onclick = () => answer(option.id);
      controls.append(button);
    }
  } else if (interaction.kind === 'confirm') {
    for (const [value, label] of [[true, interaction.config.confirm_label || '确认'], [false, interaction.config.cancel_label || '取消']] as const) {
      const button = el('button', 'interaction-option', valueText(label));
      button.type = 'button';
      button.disabled = disabled;
      button.onclick = () => answer(value);
      controls.append(button);
    }
  } else if (interaction.kind === 'slider') {
    const min = Number(interaction.config.min), max = Number(interaction.config.max);
    if (Number.isFinite(min) && Number.isFinite(max) && max > min) {
      const step = Number(interaction.config.step) > 0 ? Number(interaction.config.step) : 1;
      const range = el('input', 'interaction-slider') as HTMLInputElement;
      range.type = 'range';
      range.min = String(min);
      range.max = String(max);
      range.step = String(step);
      range.value = String(Math.max(min, Math.min(max, Number(interaction.config.default ?? min))));
      range.disabled = disabled;
      const output = el('output', 'interaction-slider-value', range.value);
      range.oninput = () => { output.value = range.value; output.textContent = range.value; };
      const submitRange = el('button', 'interaction-option', '确认');
      submitRange.type = 'button';
      submitRange.disabled = disabled;
      submitRange.onclick = () => answer(Number(range.value));
      controls.append(range, output, submitRange);
    }
  }
  if (controls.childElementCount) root.append(controls);
  return root;
}
function renderMessages(snapshot: Snapshot) {
  const signature = JSON.stringify([snapshot.messages, snapshot.ui_state?.submitting, snapshot.editing]);
  if (signature === messagesKey) return;
  messagesKey = signature;
  const follow = forceFollow || messages.scrollHeight - messages.scrollTop - messages.clientHeight < 80;
  const oldTop = messages.scrollTop;
  messages.replaceChildren();
  for (const [index, message] of (snapshot.messages || []).entries()) {
    const meta = message.metadata || {};
    const key = message.message_id || `${message.role}:${index}`;
    if (meta.message_kind === 'world_interaction') {
      const interaction = interactionFrom(meta.interaction);
      if (interaction) messages.append(renderInteraction(message.message_id || '', interaction, !!snapshot.ui_state?.submitting));
      continue;
    }
    if (['character_creation', 'switch_proposal'].includes(String(meta.message_kind))) continue;
    const isDirector = meta.action_type === 'director_trace';
    const streaming = meta.streaming === true;
    if (isDirector) {
      const text = valueText(meta.reasoning || meta.trace_text || messageText(message));
      if (text) messages.append(cot(text, '行程助手思考', key, streaming));
      continue;
    }
    const isPlayer = ['player', 'user'].includes(message.role);
    const isSystem = ['system', 'narration'].includes(message.role);
    const text = messageText(message);
    if (message.role === 'narration' || ['event', 'narration_event'].includes(String(meta.message_kind))) {
      if (text) messages.append(el('article', 'event', text));
      continue;
    }
    if (!text && !meta.reasoning && !meta.tool_activity && !messageParts(message).length) continue;
    const row = el('article', `message message--${isPlayer ? 'player' : isSystem ? 'system' : 'agent'}`);
    row.append(el('div', 'speaker', message.speaker || (isPlayer ? snapshot.player?.name || '我' : isSystem ? '提示' : '行程助手')));
    if (meta.reasoning) row.append(cot(valueText(meta.reasoning), '行程助手思考', key, streaming));
    const activity = meta.tool_activity as { status?: string; tools?: { name: string }[] } | undefined;
    if (activity) row.append(el('div', 'tool' + (activity.status === 'calling' ? ' streaming' : ''), (activity.status === 'calling' ? '正在调用：' : '已调用：') + (activity.tools || []).map((tool) => tool.name).join('、')));
    if (text) row.append(el('div', 'message-content', text));
    for (const part of messageParts(message)) {
      if (part.type === 'image_url' && part.image_url?.url) {
        const image = el('img', 'message-image') as HTMLImageElement;
        image.src = part.image_url.url;
        image.alt = '图片附件';
        row.append(image);
      } else if (part.type === 'input_audio' && part.input_audio?.data) {
        const audio = el('audio', 'message-audio') as HTMLAudioElement;
        audio.controls = true;
        audio.src = `data:audio/${part.input_audio.format || 'wav'};base64,${part.input_audio.data}`;
        row.append(audio);
      }
    }
    if (!isSystem && !message.pending) {
      const actions = el('div', 'message-actions');
      const button = (label: string, work: () => Promise<unknown> | void) => {
        const node = el('button', '', label);
        node.type = 'button';
        node.disabled = !!snapshot.ui_state?.submitting;
        node.onclick = () => { const result = work(); if (result) void result.catch(showError); };
        actions.append(node);
      };
      if (isPlayer) {
        const turn = Number(meta.turn_index);
        if (Number.isInteger(turn) && turn > 0) {
          button('编辑', async () => { await world.actions.editTurn(text, turn); input.value = text; draftDirty = true; input.focus(); });
          button('重发', () => submit({ content: text, mode: 'resend', turnIndex: turn }));
        }
      } else {
        button('复制', () => world.actions.copyText(text));
        button('分支', () => world.actions.branch());
      }
      if (actions.childElementCount) row.append(actions);
    }
    messages.append(row);
    if (meta.narration) messages.append(el('article', 'message message--system', valueText(meta.narration)));
  }
  if (!(snapshot.messages || []).length) messages.append(el('p', 'empty', '告诉行程助手接下来要做什么、什么时候做，以及希望提前多久收到提醒。'));
  if (snapshot.ui_state?.submitting) messages.append(el('div', 'thinking-indicator', '行程助手正在拆解和安排…'));
  messages.scrollTop = follow ? messages.scrollHeight : oldTop;
  forceFollow = false;
}

function switchTab(next: string) {
  tab = next;
  for (const panel of qsa<HTMLElement>('[data-tab-panel]')) panel.hidden = panel.dataset.tabPanel !== next;
  for (const button of qsa<HTMLElement>('[data-tab]')) {
    const active = button.dataset.tab === next;
    button.classList.toggle('is-active', active);
    button.setAttribute('aria-current', active ? 'page' : 'false');
  }
  if (next === 'tasks') renderTasks(state);
}
function renderTasks(snapshot: Snapshot) {
  const todo = listAttribute('todo_items');
  const done = listAttribute('completed_items');
  const valid = new Set(todo);
  for (const item of Array.from(checkedItems)) if (!valid.has(item)) checkedItems.delete(item);
  setText('todo-count', String(todo.length));
  setText('done-count', String(done.length));
  setText('tab-todo-count', todo.length ? String(todo.length) : '');
  const todoRoot = $('todo-list');
  if (todoRoot) {
    todoRoot.replaceChildren();
    const empty = $('todo-empty');
    if (empty) empty.hidden = todo.length > 0;
    for (const item of todo) {
      const label = el('label', `task-row${checkedItems.has(item) ? ' is-checked' : ''}`);
      const box = el('input') as HTMLInputElement;
      box.type = 'checkbox';
      box.checked = checkedItems.has(item);
      box.onchange = () => {
        if (box.checked) checkedItems.add(item);
        else checkedItems.delete(item);
        label.classList.toggle('is-checked', box.checked);
        renderConfirm();
      };
      const copy = el('span', 'task-text');
      copy.append(el('strong', '', item));
      label.append(box, copy);
      todoRoot.append(label);
    }
  }
  const doneRoot = $('done-list');
  if (doneRoot) {
    doneRoot.replaceChildren();
    const empty = $('done-empty');
    if (empty) empty.hidden = done.length > 0;
    for (const item of done) {
      const row = el('div', 'task-row task-row--done');
      row.append(el('span', 'task-check', '✓'), el('span', 'task-text', item));
      doneRoot.append(row);
    }
  }
  renderConfirm();
}
function renderConfirm() {
  const button = $<HTMLButtonElement>('confirm-done');
  if (!button) return;
  button.disabled = checkedItems.size === 0;
  button.textContent = checkedItems.size ? `确认完成 ${checkedItems.size} 项` : '勾选已完成的待办';
}
function render(snapshot: Snapshot) {
  state = snapshot;
  if (snapshot.draft_input?.microphone_error) localError = snapshot.draft_input.microphone_error;
  setText('world-name', snapshot.world?.name || '行程助手');
  setText('time-label', snapshot.session?.time_label || '实时');
  renderMessages(snapshot);
  renderTasks(snapshot);
  if (snapshot.editing?.turnIndex !== editingTurn) {
    editingTurn = snapshot.editing?.turnIndex;
    if (snapshot.editing) { input.value = snapshot.draft_input?.value || snapshot.editing.originalContent; draftDirty = true; }
  }
  if (!draftDirty && !sending && document.activeElement !== input) input.value = snapshot.draft_input?.value || '';
  const cancel = $('cancel-edit');
  if (cancel) cancel.hidden = !snapshot.editing;
  const voice = $('voice');
  if (voice) {
    voice.setAttribute('aria-label', snapshot.draft_input?.is_recording ? '结束录音' : '开始录音');
    voice.classList.toggle('recording', !!snapshot.draft_input?.is_recording);
  }
  const attachments = $('attachments');
  if (attachments) {
    attachments.replaceChildren();
    for (const [index, file] of (snapshot.draft_input?.audios || []).entries()) {
      const chip = el('button', 'attachment', `${file.name || '语音'} ×`);
      chip.type = 'button';
      chip.onclick = () => run(() => world.actions.removeAudio(index));
      attachments.append(chip);
    }
  }
  updateStatus();
}
let toastTimer = 0;
function toast(message: string) {
  const node = $('toast');
  if (!node) return;
  node.textContent = message;
  node.classList.add('show');
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => node.classList.remove('show'), 2200);
}
function hydrate() {
  for (const button of qsa<HTMLElement>('[data-tab]')) button.addEventListener('click', () => switchTab(button.dataset.tab || 'chat'));
  $('new-task')?.addEventListener('click', () => {
    switchTab('chat');
    if (!input.value) input.value = '帮我安排：';
    draftDirty = true;
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  });
  $('confirm-done')?.addEventListener('click', () => {
    if (!checkedItems.size) return;
    const items = Array.from(checkedItems);
    void submit({ content: `我已完成以下待办事项：${items.join('；')}` }).then((ok) => {
      if (!ok) return;
      checkedItems.clear();
      renderTasks(state);
      toast('已告诉行程助手');
    });
  });
  switchTab('chat');
}
function boot() {
  hydrate();
  world.subscribe((next) => {
    state = next;
    render(next);
  });
}
boot();
