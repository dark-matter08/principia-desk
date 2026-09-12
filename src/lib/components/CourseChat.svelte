<script lang="ts">
  import { api, type ChatMessage, type ChatOwner } from '../ipc';
  import Markdown from './Markdown.svelte';
  import { tick } from 'svelte';
  import { X, ArrowUp, RotateCcw, BookOpen, Sparkles } from 'lucide-svelte';

  // `open` is owned by the parent (a visible header button in CourseReader)
  // so the chat has an unmissable entry point instead of only a thin edge tab.
  let {
    courseId,
    classroomSessionId,
    studySessionId,
    open = $bindable(false),
  }: { courseId?: number; classroomSessionId?: number; studySessionId?: string; open?: boolean } = $props();

  let messages = $state<ChatMessage[]>([]);
  let loading = $state(false);
  let sending = $state(false);
  let error = $state('');
  let input = $state('');
  let pendingMessage = $state('');
  let liveAnnouncement = $state('');
  let listEl = $state<HTMLElement | undefined>(undefined);
  let inputEl = $state<HTMLTextAreaElement | undefined>(undefined);
  let loadedFor = '';
  const ownerId = $derived(studySessionId ?? classroomSessionId ?? courseId ?? -1);
  const classroomMode = $derived(classroomSessionId !== undefined);
  const studyMode = $derived(studySessionId !== undefined);
  const ownerKey = $derived(`${studyMode ? 'study' : classroomMode ? 'classroom' : 'course'}:${ownerId}`);
  const owner: ChatOwner = $derived(
    studyMode
      ? { study_session_id: studySessionId }
      : classroomMode
        ? { classroom_session_id: classroomSessionId }
        : { course_id: courseId },
  );
  const starterPrompts = [
    'Explain the hardest mechanism more simply.',
    'Show me a small runnable example.',
    'Connect this course to the exercise.',
    'Check whether I understand the core idea.',
  ];
  const latestAssistant = $derived(messages.filter((message) => message.role === 'assistant').at(-1));
  const suggestions = $derived(
    messages.length === 0 ? starterPrompts : (latestAssistant?.follow_ups ?? [])
  );

  $effect(() => {
    const key = ownerKey;
    if (!open || key === loadedFor) return;
    loadedFor = key;
    messages = [];
    error = '';
    loading = true;
    const capturedOwner = owner;
    api
      .getChat(capturedOwner)
      .then((m) => {
        messages = m;
        loading = false;
        scrollToBottom();
      })
      .catch((e) => {
        error = `Could not load this conversation: ${String(e)}`;
        loading = false;
      });
  });

  async function scrollToBottom() {
    await tick();
    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    listEl?.scrollTo({
      top: listEl.scrollHeight,
      behavior: reduceMotion ? 'auto' : 'smooth',
    });
  }

  function close() {
    open = false;
  }

  $effect(() => {
    if (open) {
      tick().then(() => inputEl?.focus());
    }
  });

  async function send(message: string) {
    const text = message.trim();
    if (!text || sending) return;
    error = '';
    pendingMessage = text;
    sending = true;
    input = '';
    await scrollToBottom();
    try {
      const thread = await api.sendChatMessage(owner, text);
      messages = thread;
      pendingMessage = '';
      liveAnnouncement = thread[thread.length - 1]?.content ?? '';
      await scrollToBottom();
    } catch (e) {
      error = String(e);
      liveAnnouncement = `Could not get an answer: ${error}`;
    } finally {
      sending = false;
    }
  }

  function onSubmit() {
    void send(input);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onSubmit();
    }
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') close();
  }

  function retry() {
    if (pendingMessage) void send(pendingMessage);
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

{#if open}
  <aside id="course-chat-drawer" class="chat-drawer" aria-label="course chat">
    <div class="chat-head mono">
      <div class="chat-head-text">
        <span class="chat-title"><Sparkles size={11} /> COURSE TUTOR</span>
        <span class="chat-sub">grounded in this course and its exercise · session-only</span>
      </div>
      <button class="chat-close" onclick={close} aria-label="close course chat">
        <X size={14} />
      </button>
    </div>

    <div class="chat-list" bind:this={listEl}>
      {#if loading}
        <p class="mono dim">loading…</p>
      {:else if messages.length === 0}
        <div class="empty-state">
          <BookOpen size={20} aria-hidden="true" />
          <p>Ask about a mechanism, code trace, production decision, or the exercise.</p>
          <span>The tutor will point you back to the exact section that grounds its answer.</span>
        </div>
      {/if}
      {#each messages as m}
        <div class="bubble" class:user={m.role === 'user'} class:assistant={m.role === 'assistant'}>
          {#if m.role === 'assistant'}
            <Markdown markdown={m.content} compact />
            {#if m.section}
              <div class="grounding mono">
                <BookOpen size={10} aria-hidden="true" />
                grounded in: {m.section}
              </div>
            {/if}
          {:else}
            {m.content}
          {/if}
        </div>
      {/each}
      {#if sending}
        <div class="bubble user pending">{pendingMessage}</div>
        <p class="mono dim thinking" role="status">thinking…</p>
      {/if}
      {#if error}
        <div class="chat-error" role="alert">
          <span>{error}</span>
          <button class="ghost mono-ghost small" onclick={retry}>
            <RotateCcw size={11} /> retry
          </button>
        </div>
      {/if}
      {#if !loading && !sending && !error && suggestions.length > 0}
        <div class="suggestions" aria-label="suggested course questions">
          <span class="suggestion-label mono">
            {messages.length === 0 ? 'START HERE' : 'KEEP EXPLORING'}
          </span>
          {#each suggestions as suggestion}
            <button class="suggestion" onclick={() => void send(suggestion)}>{suggestion}</button>
          {/each}
        </div>
      {/if}
    </div>

    <div class="composer">
      <div class="chat-input-row">
        <textarea
          bind:this={inputEl}
          bind:value={input}
          onkeydown={onKeydown}
          placeholder="Ask what, why, how, or show your current reasoning…"
          rows="3"
          maxlength="2000"
          aria-label="ask a question about this course"
        ></textarea>
        <button
          class="cta mono-cta send"
          onclick={onSubmit}
          disabled={sending || input.trim().length === 0}
          aria-label="send question"
        >
          <ArrowUp size={13} />
        </button>
      </div>
      <div class="composer-help mono">
        <span>Enter to send · Shift+Enter for a new line · Esc to close</span>
        <span class:near-limit={input.length > 1800}>{input.length}/2000</span>
      </div>
    </div>
    <p class="sr-only" role="status" aria-live="polite">{liveAnnouncement}</p>
  </aside>
{/if}

<style>
  .chat-drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(460px, 96vw);
    background: var(--bg);
    border-left: 1px solid var(--border);
    box-shadow: -12px 0 40px rgba(0, 0, 0, 0.25);
    z-index: 50;
    display: flex;
    flex-direction: column;
    animation: slide-in 0.25s ease;
  }
  @keyframes slide-in {
    from {
      transform: translateX(20px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  .chat-head {
    padding: 14px 12px 10px 16px;
    border-bottom: 1px solid var(--border);
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
    font-size: 10.5px;
    letter-spacing: 0.5px;
    color: var(--accent);
  }
  .chat-head-text {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .chat-title {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .chat-sub {
    color: var(--faint);
    letter-spacing: 0.3px;
  }
  .chat-close {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    padding: 6px;
    min-width: 28px;
    min-height: 28px;
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .chat-close:hover {
    color: var(--fg);
  }
  .chat-list {
    flex: 1;
    overflow-y: auto;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .dim {
    color: var(--faint);
    font-size: 12px;
    line-height: 1.5;
  }
  .empty-state {
    display: grid;
    justify-items: center;
    gap: 8px;
    padding: 28px 18px 18px;
    text-align: center;
    color: var(--muted);
  }
  .empty-state p {
    margin: 0;
    max-width: 320px;
    font-size: 13px;
    line-height: 1.55;
    color: var(--fg);
  }
  .empty-state span {
    max-width: 330px;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--faint);
  }
  .bubble {
    font-size: 13px;
    line-height: 1.55;
    padding: 9px 12px;
    border-radius: var(--radius-panel);
    max-width: 92%;
  }
  .bubble.user {
    align-self: flex-end;
    background: var(--surface-2);
    color: var(--fg);
  }
  .bubble.assistant {
    align-self: flex-start;
    background: var(--surface);
    border: 1px solid var(--border);
    color: var(--fg);
  }
  .grounding {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 8px;
    padding-top: 7px;
    border-top: 1px solid var(--border);
    color: var(--faint);
    font-size: 9.5px;
    letter-spacing: 0.25px;
  }
  .bubble.pending {
    opacity: 0.6;
  }
  .thinking {
    align-self: flex-start;
    margin: 0;
  }
  .chat-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font-size: 11.5px;
    color: var(--bad-fg);
    background: var(--bad-bg);
    border-radius: var(--radius-panel);
    padding: 8px 10px;
  }
  .chat-error .small {
    padding: 3px 9px;
    font-size: 10.5px;
    flex-shrink: 0;
  }
  .suggestions {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 7px;
    margin-top: 4px;
  }
  .suggestion-label {
    color: var(--faint);
    font-size: 9px;
    letter-spacing: 0.8px;
  }
  .suggestion {
    max-width: 94%;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-detail);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 11.5px;
    line-height: 1.35;
    text-align: left;
    cursor: pointer;
  }
  .suggestion:hover {
    border-color: var(--accent);
    color: var(--fg);
    background: var(--surface);
  }
  .chat-close:focus-visible,
  .suggestion:focus-visible,
  .send:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .composer {
    padding: 12px 16px 10px;
    border-top: 1px solid var(--border);
  }
  .chat-input-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }
  .chat-input-row textarea {
    flex: 1;
    resize: none;
    font-size: 13px;
  }
  .send {
    padding: 10px;
    flex-shrink: 0;
  }
  .composer-help {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 6px;
    color: var(--faint);
    font-size: 8.5px;
    line-height: 1.3;
  }
  .near-limit {
    color: var(--warning-fg, var(--accent));
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
  }
  @media (prefers-reduced-motion: reduce) {
    .chat-drawer {
      animation: none;
    }
    .chat-list {
      scroll-behavior: auto;
    }
  }
</style>
