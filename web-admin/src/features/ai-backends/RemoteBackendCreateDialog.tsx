import type { WriteAiBackendInput } from '@/api/ai-backends'

import type { PlacementPolicyInput } from './AiBackendEditorForm'
import AiBackendEditorDialog from './AiBackendEditorDialog'

export default function RemoteBackendCreateDialog(props: {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSubmit: (input: WriteAiBackendInput, placementPolicy?: PlacementPolicyInput) => Promise<void>
}) {
  return (
    <AiBackendEditorDialog
      open={props.open}
      initialHosting="remote"
      title="Create Remote Backend"
      description="Configure an external AI endpoint and place it onto the nodes that should route traffic."
      submitLabel="Create Remote Backend"
      onOpenChange={props.onOpenChange}
      onSubmit={props.onSubmit}
    />
  )
}
