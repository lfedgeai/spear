import type { Dispatch, SetStateAction } from 'react'

import { Input } from '@/components/ui/input'

import type { AiBackendEditorFormState } from './AiBackendEditorForm'
import {
  setMetadataBooleanField,
  setMetadataStringField,
} from './AiBackendEditorForm'

export default function LocalLlamaCppFields(props: {
  form: AiBackendEditorFormState
  setForm: Dispatch<SetStateAction<AiBackendEditorFormState>>
}) {
  const { form, setForm } = props
  return (
    <div className="space-y-3 rounded-[var(--radius)] border border-[hsl(var(--border))] p-3">
      <div>
        <div className="text-sm font-medium">Local llama.cpp Runtime</div>
        <div className="text-xs text-[hsl(var(--muted-foreground))]">
          These fields are stored in metadata and used by the node-local llama.cpp supervisor.
        </div>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <div className="text-sm font-medium">Model URL</div>
          <Input
            value={form.model_url}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                model_url: event.target.value,
                metadata: setMetadataStringField(current.metadata, 'model_url', event.target.value),
              }))
            }
            placeholder="e.g. https://host/path/model.gguf"
          />
        </div>
        <div className="space-y-1">
          <div className="text-sm font-medium">Model Path</div>
          <Input
            value={form.model_path}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                model_path: event.target.value,
                metadata: setMetadataStringField(current.metadata, 'model_path', event.target.value),
              }))
            }
            placeholder="e.g. /models/llama/model.gguf"
          />
        </div>
      </div>

      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={form.skip_download}
          onChange={(event) =>
            setForm((current) => ({
              ...current,
              skip_download: event.target.checked,
              metadata: setMetadataBooleanField(
                current.metadata,
                'skip_download',
                event.target.checked,
              ),
            }))
          }
        />
        <span>Skip Download</span>
      </label>

      <div className="grid grid-cols-3 gap-3">
        <div className="space-y-1">
          <div className="text-sm font-medium">Download Timeout (s)</div>
          <Input
            value={form.download_timeout_s}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                download_timeout_s: event.target.value,
                metadata: setMetadataStringField(
                  current.metadata,
                  'download_timeout_s',
                  event.target.value,
                ),
              }))
            }
            placeholder="3600"
          />
        </div>
        <div className="space-y-1">
          <div className="text-sm font-medium">Threads</div>
          <Input
            value={form.threads}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                threads: event.target.value,
                metadata: setMetadataStringField(current.metadata, 'threads', event.target.value),
              }))
            }
            placeholder="8"
          />
        </div>
        <div className="space-y-1">
          <div className="text-sm font-medium">Context Size</div>
          <Input
            value={form.ctx_size}
            onChange={(event) =>
              setForm((current) => ({
                ...current,
                ctx_size: event.target.value,
                metadata: setMetadataStringField(current.metadata, 'ctx_size', event.target.value),
              }))
            }
            placeholder="4096"
          />
        </div>
      </div>

      <div className="text-xs text-[hsl(var(--muted-foreground))]">
        Provide either Model URL or Model Path. Model URL is downloaded automatically unless Skip
        Download is enabled.
      </div>
    </div>
  )
}
