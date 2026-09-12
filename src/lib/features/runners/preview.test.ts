import { describe, expect, it } from 'vitest';
import { previewRunners, previewModels, previewLocal, rememberPreviewModel, desktopRequired, previewConfiguration, savePreviewConfiguration } from './preview';
import { chatCandidate, fitsInRam, OLLAMA_MODELS, sameModel } from './local-models';
import { modelPage, shortlistOptions } from './model-list';

describe('runner setup preview and local model selection', () => {
  it('represents all three routes without claiming real installation or API access', () => {
    const runners = previewRunners();
    expect(runners).toHaveLength(13);
    expect(new Set(runners.map(r => r.kind))).toEqual(new Set(['cli', 'api', 'local']));
    expect(runners.every(r => !r.available)).toBe(true);
    expect(runners.find(r => r.provider === 'ollama')?.needs_key).toBeNull();
    expect(previewModels('openai').models).toEqual([]);
    expect(previewLocal().can_install).toBe(false);
  });
  it('retains a model per runner without saving a provider credential', async () => {
    rememberPreviewModel('codex', 'my-codex-model');
    rememberPreviewModel('openai', 'my-openai-model');
    expect(previewRunners().find(r => r.provider === 'codex')?.saved_model).toBe('my-codex-model');
    await expect(desktopRequired()).rejects.toThrow('requires the desktop app');
  });
  it('reserves memory for the desktop and excludes known non-chat weights from tutor selection', () => {
    expect(sameModel('nomic-embed-text', 'nomic-embed-text:latest')).toBe(true);
    expect(chatCandidate('nomic-embed-text:latest')).toBe(false);
    expect(chatCandidate('model:cloud')).toBe(false);
    expect(chatCandidate('qwen2.5:7b')).toBe(true);
    expect(fitsInRam(OLLAMA_MODELS.find(m => m.id === 'qwen2.5:7b')!, 8)).toBe(false);
    expect(fitsInRam(OLLAMA_MODELS.find(m => m.id === 'qwen2.5:7b')!, 16)).toBe(true);
  });
  it('edits a model library independently from the selected model', () => {
    rememberPreviewModel('openai', 'active-model');
    const setup = previewConfiguration('openai');
    savePreviewConfiguration({ ...setup, models: ['different-model', 'another-model'] });
    expect(previewRunners().find(r => r.provider === 'openai')?.saved_model).toBe('active-model');
    expect(previewConfiguration('openai').models).toEqual(['different-model', 'another-model']);
    setup.models.push('unsaved-edit');
    expect(previewConfiguration('openai').models).not.toContain('unsaved-edit');
    expect(shortlistOptions(['different-model'], 'active-model').map(m => m.value)).toEqual(['active-model', 'different-model']);
  });
  it('keeps long catalogues bounded and makes filtered results reachable', () => {
    const models = Array.from({ length: 123 }, (_, i) => ({ id: `vendor/model-${i}`, label: `Model ${i}`, free: i % 2 === 0, input_usd_per_million: null, output_usd_per_million: null, context_length: null, tools: false, json_mode: false }));
    expect(modelPage(models, '', 0).items).toHaveLength(5);
    expect(modelPage(models, '', 24).items.map(m => m.id)).toEqual(['vendor/model-120','vendor/model-121','vendor/model-122']);
    const filtered = modelPage(models, 'vendor 122', 24, true);
    expect(filtered.current).toBe(0);
    expect(filtered.items[0].id).toBe('vendor/model-122');
    expect(modelPage(models, 'unmatched', 0).count).toBe(0);
  });
});
