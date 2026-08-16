import type { WriteAiBackendInput } from '@/api/ai-backends'

import type { PlacementPolicyInput } from './AiBackendEditorForm'
import AiBackendEditorDialog from './AiBackendEditorDialog'

export default function LocalBackendCreateDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: (input: WriteAiBackendInput, placementPolicy?: PlacementPolicyInput) => Promise<void>
}) {
  return (
    <AiBackendEditorDialog
      open={props.open}
      initialHosting="local"
      title="Create Local Backend"
      description="Configure a node-local runtime such as llama.cpp or vLLM and place it onto the target node."
      submitLabel="Create Local Backend"
      onOpenChange={props.onOpenChange}
      onSubmit={props.onSubmit}
    />
  )
}
